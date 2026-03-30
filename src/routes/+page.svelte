<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  // Types
  interface Device {
    name: string;
    address: string;
    model?: string;
  }

  interface Capabilities {
    model: string;
    max_ambient_level: number;
    supports_focus_on_voice: boolean;
    supports_speak_to_chat: boolean;
    supports_adaptive_nc: boolean;
    supports_equalizer: boolean;
    supports_dsee: boolean;
    supports_vpt: boolean;
    supports_sound_position: boolean;
    dual_battery: boolean;
  }

  interface BatteryInfo {
    level: number;
    charging: boolean;
    right_level?: number;
    right_charging?: boolean;
    case_level?: number;
    case_charging?: boolean;
  }

  interface CommandResult {
    success: boolean;
    message: string;
  }

  // State
  let devices = $state<Device[]>([]);
  let selectedDevice = $state<string | null>(null);
  let connectedDevice = $state<Device | null>(null);
  let capabilities = $state<Capabilities | null>(null);
  let battery = $state<BatteryInfo | null>(null);
  let isScanning = $state(false);
  let isConnecting = $state(false);
  let error = $state<string | null>(null);

  // ANC State
  let ancMode = $state<"off" | "nc" | "ambient">("off");
  let ambientLevel = $state(10);
  let focusOnVoice = $state(false);

  // Volume State
  let volume = $state(20);

  // EQ State
  let eqPreset = $state("off");
  let eqBands = $state([10, 10, 10, 10, 10]); // 5-band EQ, 0-20 range

  // Speak-to-Chat State
  let speakToChatEnabled = $state(false);
  let speakToChatSensitivity = $state("medium");
  let speakToChatTimeout = $state(15);

  // DSEE State
  let dseeEnabled = $state(false);

  // VPT/Sound Position State
  let vptPreset = $state("off");
  let soundPosition = $state("off");

  let connectionStatus = $state("Initializing...");

  // Initialize and auto-connect
  onMount(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.onCloseRequested(async (event) => {
        event.preventDefault();
        await appWindow.hide();
      });

      // Listen for backend auto-connect events
      listen("device-connected", async () => {
        await checkConnection();
      });

      // Check if backend already connected (auto_connect may have finished)
      connectionStatus = "Checking connection...";
      await checkConnection();

      if (connectedDevice) {
        connectionStatus = `Connected to ${connectedDevice.name}`;
        return;
      }

      // Not connected yet — initialize bluetooth and auto-connect from frontend
      connectionStatus = "Initializing Bluetooth...";
      await invoke("init_bluetooth");

      // Poll for backend auto-connect (it runs in parallel)
      for (let i = 0; i < 10; i++) {
        await new Promise(r => setTimeout(r, 1500));
        await checkConnection();
        if (connectedDevice) {
          connectionStatus = `Connected to ${connectedDevice.name}`;
          return;
        }
        connectionStatus = `Waiting for headphones... (${i + 1}/10)`;
      }

      // Still not connected — let user manually scan
      connectionStatus = "Not connected";
    } catch (e) {
      error = `Failed to initialize: ${e}`;
      connectionStatus = "Error";
    }
  });

  async function checkConnection() {
    const status = await invoke<Device | null>("get_connection_status");
    connectedDevice = status;
    if (status) {
      await fetchCapabilities();
    }
  }

  async function fetchCapabilities() {
    try {
      capabilities = await invoke<Capabilities | null>("get_capabilities");
    } catch (e) {
      console.error("Failed to fetch capabilities:", e);
    }
  }

  async function fetchBattery() {
    try {
      battery = await invoke<BatteryInfo | null>("get_battery_status");
    } catch (e) {
      console.error("Failed to fetch battery:", e);
    }
  }

  // Poll battery status
  let batteryInterval: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    if (connectedDevice) {
      fetchBattery();
      batteryInterval = setInterval(fetchBattery, 30000);
    } else {
      battery = null;
      if (batteryInterval) {
        clearInterval(batteryInterval);
        batteryInterval = null;
      }
    }
    return () => {
      if (batteryInterval) clearInterval(batteryInterval);
    };
  });

  async function scanDevices() {
    isScanning = true;
    error = null;
    try {
      devices = await invoke<Device[]>("discover_devices");
    } catch (e) {
      error = `Scan failed: ${e}`;
    } finally {
      isScanning = false;
    }
  }

  async function connect() {
    if (!selectedDevice) return;
    isConnecting = true;
    error = null;
    try {
      const device = devices.find(d => d.address === selectedDevice);
      const result = await invoke<CommandResult>("connect_device", {
        address: selectedDevice,
        name: device?.name ?? null,
      });
      if (result.success) {
        await checkConnection();
      } else {
        error = result.message;
      }
    } catch (e) {
      error = `Connection failed: ${e}`;
    } finally {
      isConnecting = false;
    }
  }

  async function disconnect() {
    try {
      await invoke<CommandResult>("disconnect_device");
      connectedDevice = null;
      capabilities = null;
      selectedDevice = null;
    } catch (e) {
      error = `Disconnect failed: ${e}`;
    }
  }

  async function setAncMode() {
    if (!connectedDevice) {
      error = "Not connected — waiting for headphones";
      return;
    }
    try {
      error = null;
      const result = await invoke<CommandResult>("set_anc_mode", {
        mode: ancMode,
        level: ancMode === "ambient" ? ambientLevel : null,
        focusOnVoice: ancMode === "ambient" ? focusOnVoice : null,
      });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `ANC failed: ${e}`;
    }
  }

  async function setEqualizer() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_equalizer", { preset: eqPreset });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set EQ: ${e}`;
    }
  }

  async function setSpeakToChat() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_speak_to_chat", {
        enabled: speakToChatEnabled,
        sensitivity: speakToChatSensitivity,
        timeout: speakToChatTimeout,
      });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set speak-to-chat: ${e}`;
    }
  }

  async function setDsee() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_dsee", { enabled: dseeEnabled });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set DSEE: ${e}`;
    }
  }

  async function setVptPreset() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_vpt_preset", { preset: vptPreset });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set VPT: ${e}`;
    }
  }

  async function setSoundPosition() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_sound_position", { position: soundPosition });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set position: ${e}`;
    }
  }

  async function setVolume() {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_volume", { level: volume });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Failed to set volume: ${e}`;
    }
  }

  async function playbackAction(action: string) {
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("playback_control", { action });
      if (!result.success) error = result.message;
    } catch (e) {
      error = `Playback failed: ${e}`;
    }
  }

  function getBatteryClass(level: number): string {
    if (level > 50) return 'high';
    if (level > 20) return 'medium';
    return 'low';
  }
</script>

<main class="min-h-screen p-4 md:p-6 max-w-2xl mx-auto">
  <!-- Header -->
  <header class="flex items-center justify-between mb-6 animate-fade-in">
    <div>
      <h1 class="text-xl font-semibold text-[var(--color-text-primary)]">
        {#if connectedDevice}
          {connectedDevice.name}
        {:else}
          Sony Headphones
        {/if}
      </h1>
      <p class="text-sm text-[var(--color-text-muted)]">
        {#if connectedDevice}
          {capabilities?.model || 'Connected'}
        {:else}
          {connectionStatus}
        {/if}
      </p>
    </div>

    {#if connectedDevice && battery}
      <div class="flex items-center gap-3">
        <div class="flex items-center gap-2 px-3 py-1.5 rounded-full bg-[var(--color-bg-card)] border border-[var(--color-border)]">
          <svg class="w-4 h-4 text-[var(--color-text-secondary)]" fill="currentColor" viewBox="0 0 24 24">
            <path d="M17 4h-3V2h-4v2H7C5.9 4 5 4.9 5 6v16c0 1.1.9 2 2 2h10c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2z"/>
          </svg>
          <span class="text-sm font-medium text-[var(--color-text-primary)]">{battery.level}%</span>
          {#if battery.charging}
            <span class="text-[var(--color-accent)]">
              <svg class="w-3 h-3" fill="currentColor" viewBox="0 0 24 24">
                <path d="M11 21h-1l1-7H7.5c-.58 0-.57-.32-.38-.66l4.5-7.96c.17-.28.65-.3.84-.02.1.15.07.34-.05.5L9.5 12H14l-1 7H14.5c.58 0 .57.32.38.66l-4.5 7.96c-.17.28-.65.3-.84.02-.1-.15-.07-.34.05-.5L11 21z"/>
              </svg>
            </span>
          {/if}
        </div>
      </div>
    {/if}
  </header>

  <!-- Error Banner -->
  {#if error}
    <div class="mb-4 p-3 rounded-xl bg-[var(--color-error)]/10 border border-[var(--color-error)]/30 animate-fade-in">
      <div class="flex items-center justify-between">
        <p class="text-sm text-[var(--color-error)]">{error}</p>
        <button
          class="text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)] transition-colors"
          onclick={() => error = null}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/>
          </svg>
        </button>
      </div>
    </div>
  {/if}

  {#if !connectedDevice}
    <!-- Connection Card -->
    <section class="card animate-fade-in">
      <h2 class="text-sm font-medium text-[var(--color-text-secondary)] mb-4">Connect Device</h2>

      {#if devices.length > 0}
        <div class="space-y-2 mb-4">
          {#each devices as device}
            <label
              class="flex items-center gap-3 p-3 rounded-xl cursor-pointer transition-all
                     {selectedDevice === device.address
                       ? 'bg-[var(--color-accent)]/10 border border-[var(--color-accent)]/30'
                       : 'bg-[var(--color-bg-secondary)] border border-transparent hover:border-[var(--color-border)]'}"
            >
              <input
                type="radio"
                name="device"
                value={device.address}
                bind:group={selectedDevice}
                class="w-4 h-4 accent-[var(--color-accent)]"
              />
              <div class="flex-1">
                <p class="text-sm font-medium text-[var(--color-text-primary)]">{device.name}</p>
                <p class="text-xs text-[var(--color-text-muted)]">{device.address}</p>
              </div>
              {#if device.model}
                <span class="text-xs px-2 py-0.5 rounded-full bg-[var(--color-bg-card)] text-[var(--color-text-secondary)]">
                  {device.model}
                </span>
              {/if}
            </label>
          {/each}
        </div>
      {/if}

      <div class="flex gap-3">
        <button
          class="flex-1 px-4 py-3 text-sm font-medium rounded-xl transition-all
                 bg-[var(--color-bg-secondary)] hover:bg-[var(--color-bg-card-hover)]
                 border border-[var(--color-border)] hover:border-[var(--color-border-subtle)]
                 disabled:opacity-50 disabled:cursor-not-allowed"
          onclick={scanDevices}
          disabled={isScanning}
        >
          {#if isScanning}
            <span class="flex items-center justify-center gap-2">
              <svg class="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path>
              </svg>
              Scanning...
            </span>
          {:else}
            Scan for Devices
          {/if}
        </button>

        {#if selectedDevice}
          <button
            class="flex-1 px-4 py-3 text-sm font-medium rounded-xl transition-all
                   bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white
                   shadow-lg shadow-[var(--color-accent-glow)]
                   disabled:opacity-50 disabled:cursor-not-allowed"
            onclick={connect}
            disabled={isConnecting}
          >
            {isConnecting ? "Connecting..." : "Connect"}
          </button>
        {/if}
      </div>
    </section>
  {:else}
    <!-- Connected State -->
    <div class="space-y-4">
      <!-- Noise Control Card -->
      <section class="card animate-fade-in">
        <h2 class="text-sm font-medium text-[var(--color-text-secondary)] mb-4">Noise Control</h2>

        <div class="flex gap-2 mb-4">
          <button
            class="flex-1 py-3 px-4 rounded-xl font-medium text-sm transition-all glow-button mode-button-off {ancMode === 'off' ? 'active' : ''}"
            onclick={() => { ancMode = 'off'; setAncMode(); }}
          >
            Off
          </button>
          <button
            class="flex-1 py-3 px-4 rounded-xl font-medium text-sm transition-all glow-button mode-button-nc {ancMode === 'nc' ? 'active' : ''}"
            onclick={() => { ancMode = 'nc'; setAncMode(); }}
          >
            <span class="flex items-center justify-center gap-2">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"/>
              </svg>
              NC
            </span>
          </button>
          <button
            class="flex-1 py-3 px-4 rounded-xl font-medium text-sm transition-all glow-button mode-button-ambient {ancMode === 'ambient' ? 'active' : ''}"
            onclick={() => { ancMode = 'ambient'; setAncMode(); }}
          >
            <span class="flex items-center justify-center gap-2">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.536 8.464a5 5 0 010 7.072m2.828-9.9a9 9 0 010 12.728M5.586 15.536a5 5 0 001.414 1.414m2.828-9.9a9 9 0 0112.728 0"/>
              </svg>
              Ambient
            </span>
          </button>
        </div>

        {#if ancMode === 'ambient'}
          <div class="space-y-4 pt-4 border-t border-[var(--color-border)]">
            <div>
              <div class="flex items-center justify-between mb-2">
                <span class="text-sm text-[var(--color-text-secondary)]">Ambient Level</span>
                <span class="text-sm font-medium text-[var(--color-accent)]">{ambientLevel}</span>
              </div>
              <input
                type="range"
                min="0"
                max={capabilities?.max_ambient_level ?? 20}
                bind:value={ambientLevel}
                onchange={setAncMode}
                class="styled-slider"
              />
            </div>

            {#if capabilities?.supports_focus_on_voice !== false}
              <label class="flex items-center justify-between cursor-pointer">
                <span class="text-sm text-[var(--color-text-secondary)]">Focus on Voice</span>
                <div
                  class="toggle {focusOnVoice ? 'active' : ''}"
                  onclick={() => { focusOnVoice = !focusOnVoice; setAncMode(); }}
                  role="switch"
                  aria-checked={focusOnVoice}
                ></div>
              </label>
            {/if}
          </div>
        {/if}
      </section>

      <!-- Volume Card -->
      <section class="card animate-fade-in">
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Volume</h2>
          <span class="text-lg font-semibold text-[var(--color-text-primary)]">{volume}</span>
        </div>
        <input
          type="range"
          min="0"
          max="30"
          bind:value={volume}
          onchange={setVolume}
          class="styled-slider"
        />
        <div class="flex justify-between mt-1 text-xs text-[var(--color-text-muted)]">
          <span>0</span>
          <span>30</span>
        </div>

        <!-- Playback Controls -->
        <div class="flex items-center justify-center gap-4 mt-6 pt-4 border-t border-[var(--color-border)]">
          <button
            class="playback-button"
            onclick={() => playbackAction('prev')}
            title="Previous Track"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 6h2v12H6zm3.5 6l8.5 6V6z"/>
            </svg>
          </button>
          <button
            class="playback-button primary"
            onclick={() => playbackAction('play')}
            title="Play"
          >
            <svg class="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z"/>
            </svg>
          </button>
          <button
            class="playback-button"
            onclick={() => playbackAction('pause')}
            title="Pause"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/>
            </svg>
          </button>
          <button
            class="playback-button"
            onclick={() => playbackAction('next')}
            title="Next Track"
          >
            <svg class="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z"/>
            </svg>
          </button>
        </div>
      </section>

      <!-- Equalizer Card -->
      {#if capabilities?.supports_equalizer !== false}
        <section class="card animate-fade-in">
          <h2 class="text-sm font-medium text-[var(--color-text-secondary)] mb-4">Equalizer</h2>

          <!-- EQ Visualization -->
          <div class="flex items-end justify-center gap-2 h-16 mb-4 px-4">
            {#each eqBands as band, i}
              <div
                class="eq-bar"
                style="height: {20 + band * 2}px"
              ></div>
            {/each}
          </div>

          <select
            bind:value={eqPreset}
            onchange={setEqualizer}
            class="select w-full"
          >
            <optgroup label="Off">
              <option value="off">Off</option>
            </optgroup>
            <optgroup label="Genre Presets">
              <option value="rock">Rock</option>
              <option value="pop">Pop</option>
              <option value="jazz">Jazz</option>
              <option value="dance">Dance</option>
              <option value="edm">EDM</option>
              <option value="rnb">R&B / Hip-Hop</option>
              <option value="acoustic">Acoustic</option>
            </optgroup>
            <optgroup label="Sound Profiles">
              <option value="bright">Bright</option>
              <option value="excited">Excited</option>
              <option value="mellow">Mellow</option>
              <option value="relaxed">Relaxed</option>
              <option value="vocal">Vocal</option>
              <option value="treble">Treble Boost</option>
              <option value="bass">Bass Boost</option>
              <option value="speech">Speech</option>
            </optgroup>
          </select>
        </section>
      {/if}

      <!-- Sound Settings Card -->
      <section class="card animate-fade-in">
        <h2 class="text-sm font-medium text-[var(--color-text-secondary)] mb-4">Sound Settings</h2>

        <div class="space-y-4">
          <!-- DSEE -->
          {#if capabilities?.supports_dsee !== false}
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm text-[var(--color-text-primary)]">DSEE Extreme</p>
                <p class="text-xs text-[var(--color-text-muted)]">Audio upsampling</p>
              </div>
              <div
                class="toggle {dseeEnabled ? 'active' : ''}"
                onclick={() => { dseeEnabled = !dseeEnabled; setDsee(); }}
                role="switch"
                aria-checked={dseeEnabled}
              ></div>
            </div>
          {/if}

          <!-- VPT -->
          {#if capabilities?.supports_vpt !== false}
            <div class="pt-4 border-t border-[var(--color-border)]">
              <p class="text-sm text-[var(--color-text-primary)] mb-2">Surround (VPT)</p>
              <select
                bind:value={vptPreset}
                onchange={setVptPreset}
                class="select w-full text-sm"
              >
                <option value="off">Off</option>
                <option value="outdoor">Outdoor Festival</option>
                <option value="arena">Arena</option>
                <option value="concert">Concert Hall</option>
                <option value="club">Club</option>
              </select>
            </div>
          {/if}

          <!-- Sound Position -->
          {#if capabilities?.supports_sound_position !== false}
            <div class="pt-4 border-t border-[var(--color-border)]">
              <p class="text-sm text-[var(--color-text-primary)] mb-2">Sound Position</p>
              <select
                bind:value={soundPosition}
                onchange={setSoundPosition}
                class="select w-full text-sm"
              >
                <option value="off">Off</option>
                <option value="front">Front</option>
                <option value="front_left">Front Left</option>
                <option value="front_right">Front Right</option>
                <option value="rear_left">Rear Left</option>
                <option value="rear_right">Rear Right</option>
              </select>
            </div>
          {/if}
        </div>
      </section>

      <!-- Speak-to-Chat Card -->
      {#if capabilities?.supports_speak_to_chat}
        <section class="card animate-fade-in">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Speak-to-Chat</h2>
              <p class="text-xs text-[var(--color-text-muted)]">Auto-pause when speaking</p>
            </div>
            <div
              class="toggle {speakToChatEnabled ? 'active' : ''}"
              onclick={() => { speakToChatEnabled = !speakToChatEnabled; setSpeakToChat(); }}
              role="switch"
              aria-checked={speakToChatEnabled}
            ></div>
          </div>

          {#if speakToChatEnabled}
            <div class="space-y-4 pt-4 border-t border-[var(--color-border)]">
              <div>
                <p class="text-sm text-[var(--color-text-secondary)] mb-2">Sensitivity</p>
                <div class="flex gap-2">
                  {#each ['low', 'medium', 'high'] as sens}
                    <button
                      class="flex-1 py-2 px-3 text-sm rounded-lg transition-all capitalize
                             {speakToChatSensitivity === sens
                               ? 'bg-[var(--color-accent)] text-white shadow-lg shadow-[var(--color-accent-glow)]'
                               : 'bg-[var(--color-bg-secondary)] text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-card-hover)]'}"
                      onclick={() => { speakToChatSensitivity = sens; setSpeakToChat(); }}
                    >
                      {sens}
                    </button>
                  {/each}
                </div>
              </div>

              <div>
                <p class="text-sm text-[var(--color-text-secondary)] mb-2">Auto-close</p>
                <select
                  bind:value={speakToChatTimeout}
                  onchange={setSpeakToChat}
                  class="select w-full text-sm"
                >
                  <option value={0}>Never</option>
                  <option value={5}>Short (~5s)</option>
                  <option value={15}>Standard (~15s)</option>
                  <option value={30}>Long (~30s)</option>
                </select>
              </div>
            </div>
          {/if}
        </section>
      {/if}

      <!-- Battery Details (for earbuds) -->
      {#if battery && capabilities?.dual_battery}
        <section class="card animate-fade-in">
          <h2 class="text-sm font-medium text-[var(--color-text-secondary)] mb-4">Battery</h2>

          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <span class="text-sm text-[var(--color-text-secondary)]">Left</span>
              <div class="flex items-center gap-2">
                <div class="w-24 battery-bar">
                  <div class="battery-fill {getBatteryClass(battery.level)}" style="width: {battery.level}%"></div>
                </div>
                <span class="text-sm font-medium text-[var(--color-text-primary)] w-10">{battery.level}%</span>
              </div>
            </div>

            {#if battery.right_level != null}
              <div class="flex items-center justify-between">
                <span class="text-sm text-[var(--color-text-secondary)]">Right</span>
                <div class="flex items-center gap-2">
                  <div class="w-24 battery-bar">
                    <div class="battery-fill {getBatteryClass(battery.right_level)}" style="width: {battery.right_level}%"></div>
                  </div>
                  <span class="text-sm font-medium text-[var(--color-text-primary)] w-10">{battery.right_level}%</span>
                </div>
              </div>
            {/if}

            {#if battery.case_level != null}
              <div class="flex items-center justify-between">
                <span class="text-sm text-[var(--color-text-secondary)]">Case</span>
                <div class="flex items-center gap-2">
                  <div class="w-24 battery-bar">
                    <div class="battery-fill {getBatteryClass(battery.case_level)}" style="width: {battery.case_level}%"></div>
                  </div>
                  <span class="text-sm font-medium text-[var(--color-text-primary)] w-10">{battery.case_level}%</span>
                </div>
              </div>
            {/if}
          </div>
        </section>
      {/if}

      <!-- Disconnect Button -->
      <button
        class="w-full py-3 px-4 text-sm font-medium rounded-xl transition-all
               bg-[var(--color-bg-card)] hover:bg-[var(--color-bg-card-hover)]
               border border-[var(--color-border)] hover:border-[var(--color-error)]/30
               text-[var(--color-text-secondary)] hover:text-[var(--color-error)]"
        onclick={disconnect}
      >
        Disconnect
      </button>
    </div>
  {/if}

  <!-- Footer -->
  <footer class="mt-8 text-center">
    <p class="text-xs text-[var(--color-text-muted)]">
      Not affiliated with Sony. Use at your own risk.
    </p>
  </footer>
</main>
