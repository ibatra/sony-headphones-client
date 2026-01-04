<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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

  // VPT State
  let vptPreset = $state("off");
  let soundPosition = $state("off");

  // EQ State
  let eqPreset = $state("off");

  // Speak-to-Chat State (XM5/XM6)
  let speakToChatEnabled = $state(false);
  let speakToChatSensitivity = $state("medium");
  let speakToChatTimeout = $state(15);

  // DSEE State
  let dseeEnabled = $state(false);

  // Initialize
  onMount(async () => {
    try {
      await invoke("init_bluetooth");
      await checkConnection();

      // Minimize to tray instead of closing
      const appWindow = getCurrentWindow();
      await appWindow.onCloseRequested(async (event) => {
        event.preventDefault();
        await appWindow.hide();
      });
    } catch (e) {
      error = `Failed to initialize: ${e}`;
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

  // Poll battery status every 30 seconds when connected
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
      if (batteryInterval) {
        clearInterval(batteryInterval);
      }
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
      // Find the device name for proper model detection
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
    if (!connectedDevice) return;
    try {
      const result = await invoke<CommandResult>("set_anc_mode", {
        mode: ancMode,
        level: ancMode === "ambient" ? ambientLevel : null,
        focusOnVoice: ancMode === "ambient" ? focusOnVoice : null,
      });
      if (!result.success) {
        error = result.message;
      }
    } catch (e) {
      error = `Failed to set ANC: ${e}`;
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

</script>

<main class="max-w-md mx-auto p-6 space-y-6">
  <!-- Header -->
  <header class="text-center space-y-2">
    <h1 class="text-2xl font-semibold text-[var(--color-text-primary)]">
      Sony Headphones
    </h1>
    <p class="text-sm text-[var(--color-text-muted)]">
      Control your WH/WF series headphones
    </p>
  </header>

  <!-- Error Banner -->
  {#if error}
    <div class="bg-[var(--color-error)]/10 border border-[var(--color-error)]/20 rounded-lg p-3">
      <p class="text-sm text-[var(--color-error)]">{error}</p>
      <button
        class="text-xs text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)] mt-1"
        onclick={() => error = null}
      >
        Dismiss
      </button>
    </div>
  {/if}

  <!-- Connection Card -->
  <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
    <div class="flex items-center justify-between">
      <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Device</h2>
      {#if connectedDevice}
        <span class="flex items-center gap-2 text-xs text-[var(--color-success)]">
          <span class="w-2 h-2 bg-[var(--color-success)] rounded-full animate-pulse"></span>
          Connected
        </span>
      {:else}
        <span class="text-xs text-[var(--color-text-muted)]">Not connected</span>
      {/if}
    </div>

    {#if connectedDevice}
      <div class="bg-[var(--color-bg-tertiary)] rounded-lg p-3 space-y-3">
        <div class="flex items-center justify-between">
          <div>
            <p class="font-medium text-[var(--color-text-primary)]">{connectedDevice.name}</p>
            <p class="text-xs text-[var(--color-text-muted)]">
              {connectedDevice.model || connectedDevice.address}
            </p>
          </div>
          <button
            class="px-3 py-1.5 text-sm bg-[var(--color-bg-primary)] hover:bg-[var(--color-border)] rounded-lg transition-colors"
            onclick={disconnect}
          >
            Disconnect
          </button>
        </div>

        <!-- Battery Status -->
        {#if battery}
          <div class="flex items-center gap-4 pt-2 border-t border-[var(--color-border)]">
            {#if capabilities?.dual_battery}
              <!-- Dual battery (earbuds) -->
              <div class="flex items-center gap-2">
                <span class="text-xs text-[var(--color-text-muted)]">L</span>
                <div class="flex items-center gap-1">
                  <div class="w-6 h-3 border border-[var(--color-border)] rounded-sm relative overflow-hidden">
                    <div
                      class="absolute inset-y-0 left-0 {battery.level > 20 ? 'bg-[var(--color-success)]' : 'bg-[var(--color-error)]'}"
                      style="width: {battery.level}%"
                    ></div>
                  </div>
                  <span class="text-xs text-[var(--color-text-secondary)]">{battery.level}%</span>
                  {#if battery.charging}
                    <span class="text-xs text-[var(--color-accent)]">+</span>
                  {/if}
                </div>
              </div>
              {#if battery.right_level != null}
                <div class="flex items-center gap-2">
                  <span class="text-xs text-[var(--color-text-muted)]">R</span>
                  <div class="flex items-center gap-1">
                    <div class="w-6 h-3 border border-[var(--color-border)] rounded-sm relative overflow-hidden">
                      <div
                        class="absolute inset-y-0 left-0 {battery.right_level > 20 ? 'bg-[var(--color-success)]' : 'bg-[var(--color-error)]'}"
                        style="width: {battery.right_level}%"
                      ></div>
                    </div>
                    <span class="text-xs text-[var(--color-text-secondary)]">{battery.right_level}%</span>
                    {#if battery.right_charging}
                      <span class="text-xs text-[var(--color-accent)]">+</span>
                    {/if}
                  </div>
                </div>
              {/if}
              {#if battery.case_level != null}
                <div class="flex items-center gap-2">
                  <span class="text-xs text-[var(--color-text-muted)]">Case</span>
                  <div class="flex items-center gap-1">
                    <div class="w-6 h-3 border border-[var(--color-border)] rounded-sm relative overflow-hidden">
                      <div
                        class="absolute inset-y-0 left-0 {battery.case_level > 20 ? 'bg-[var(--color-success)]' : 'bg-[var(--color-error)]'}"
                        style="width: {battery.case_level}%"
                      ></div>
                    </div>
                    <span class="text-xs text-[var(--color-text-secondary)]">{battery.case_level}%</span>
                    {#if battery.case_charging}
                      <span class="text-xs text-[var(--color-accent)]">+</span>
                    {/if}
                  </div>
                </div>
              {/if}
            {:else}
              <!-- Single battery (over-ear) -->
              <div class="flex items-center gap-2 flex-1">
                <svg class="w-4 h-4 text-[var(--color-text-muted)]" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M17 4h-3V2h-4v2H7C5.9 4 5 4.9 5 6v16c0 1.1.9 2 2 2h10c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2z"/>
                </svg>
                <div class="flex-1 h-2 bg-[var(--color-bg-primary)] rounded-full overflow-hidden">
                  <div
                    class="h-full {battery.level > 20 ? 'bg-[var(--color-success)]' : 'bg-[var(--color-error)]'} transition-all"
                    style="width: {battery.level}%"
                  ></div>
                </div>
                <span class="text-sm font-medium text-[var(--color-text-secondary)]">{battery.level}%</span>
                {#if battery.charging}
                  <span class="text-xs text-[var(--color-accent)]">Charging</span>
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {:else}
      <!-- Device List -->
      {#if devices.length > 0}
        <div class="space-y-2">
          {#each devices as device}
            <label
              class="flex items-center gap-3 p-3 bg-[var(--color-bg-tertiary)] rounded-lg cursor-pointer hover:bg-[var(--color-border)] transition-colors"
            >
              <input
                type="radio"
                name="device"
                value={device.address}
                bind:group={selectedDevice}
                class="w-4 h-4 accent-[var(--color-accent)]"
              />
              <div>
                <p class="text-sm font-medium text-[var(--color-text-primary)]">{device.name}</p>
                <p class="text-xs text-[var(--color-text-muted)]">
                  {device.model || device.address}
                </p>
              </div>
            </label>
          {/each}
        </div>
      {/if}

      <div class="flex gap-2">
        <button
          class="flex-1 px-4 py-2.5 text-sm bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-border)] rounded-lg transition-colors disabled:opacity-50"
          onclick={scanDevices}
          disabled={isScanning}
        >
          {isScanning ? "Scanning..." : "Scan Devices"}
        </button>
        {#if selectedDevice}
          <button
            class="flex-1 px-4 py-2.5 text-sm bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white rounded-lg transition-colors disabled:opacity-50"
            onclick={connect}
            disabled={isConnecting}
          >
            {isConnecting ? "Connecting..." : "Connect"}
          </button>
        {/if}
      </div>
    {/if}
  </section>

  <!-- ANC Controls -->
  {#if connectedDevice}
    <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
      <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Noise Control</h2>

      <!-- Mode Selector -->
      <div class="grid grid-cols-3 gap-2">
        <button
          class="px-3 py-2.5 text-sm rounded-lg transition-colors {ancMode === 'off' ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-border)]'}"
          onclick={() => { ancMode = 'off'; setAncMode(); }}
        >
          Off
        </button>
        <button
          class="px-3 py-2.5 text-sm rounded-lg transition-colors {ancMode === 'nc' ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-border)]'}"
          onclick={() => { ancMode = 'nc'; setAncMode(); }}
        >
          Noise Cancel
        </button>
        <button
          class="px-3 py-2.5 text-sm rounded-lg transition-colors {ancMode === 'ambient' ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-border)]'}"
          onclick={() => { ancMode = 'ambient'; setAncMode(); }}
        >
          Ambient
        </button>
      </div>

      <!-- Ambient Level Slider -->
      {#if ancMode === 'ambient'}
        <div class="space-y-3 pt-2">
          <div class="flex items-center justify-between">
            <label class="text-sm text-[var(--color-text-secondary)]">Ambient Level</label>
            <span class="text-sm font-medium text-[var(--color-text-primary)]">{ambientLevel}</span>
          </div>
          <input
            type="range"
            min="0"
            max={capabilities?.max_ambient_level ?? 20}
            bind:value={ambientLevel}
            onchange={setAncMode}
            class="w-full h-2 bg-[var(--color-bg-tertiary)] rounded-lg appearance-none cursor-pointer accent-[var(--color-accent)]"
          />

          {#if capabilities?.supports_focus_on_voice !== false}
            <label class="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                bind:checked={focusOnVoice}
                onchange={setAncMode}
                class="w-4 h-4 rounded accent-[var(--color-accent)]"
              />
              <span class="text-sm text-[var(--color-text-secondary)]">Focus on Voice</span>
            </label>
          {/if}
        </div>
      {/if}
    </section>

    <!-- Speak-to-Chat (XM5/XM6 only) -->
    {#if capabilities?.supports_speak_to_chat}
      <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
        <div class="flex items-center justify-between">
          <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Speak-to-Chat</h2>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              bind:checked={speakToChatEnabled}
              onchange={setSpeakToChat}
              class="sr-only peer"
            />
            <div class="w-11 h-6 bg-[var(--color-bg-tertiary)] peer-focus:ring-2 peer-focus:ring-[var(--color-accent)] rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-[var(--color-accent)]"></div>
          </label>
        </div>

        {#if speakToChatEnabled}
          <div class="space-y-3">
            <div>
              <label class="text-sm text-[var(--color-text-secondary)] block mb-2">Sensitivity</label>
              <div class="grid grid-cols-3 gap-2">
                {#each ['low', 'medium', 'high'] as sens}
                  <button
                    class="px-3 py-2 text-sm rounded-lg transition-colors capitalize {speakToChatSensitivity === sens ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-tertiary)] hover:bg-[var(--color-border)]'}"
                    onclick={() => { speakToChatSensitivity = sens; setSpeakToChat(); }}
                  >
                    {sens}
                  </button>
                {/each}
              </div>
            </div>

            <div>
              <label class="text-sm text-[var(--color-text-secondary)] block mb-2">Auto-close</label>
              <select
                bind:value={speakToChatTimeout}
                onchange={setSpeakToChat}
                class="w-full px-3 py-2.5 bg-[var(--color-bg-tertiary)] text-[var(--color-text-primary)] rounded-lg border border-[var(--color-border)] focus:outline-none focus:border-[var(--color-accent)]"
              >
                <option value={0}>Never</option>
                <option value={5}>Short (5s)</option>
                <option value={10}>Standard (10s)</option>
                <option value={15}>Long (15s)</option>
              </select>
            </div>
          </div>
        {/if}
      </section>
    {/if}

    <!-- EQ Controls -->
    {#if capabilities?.supports_equalizer !== false}
      <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
        <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Equalizer</h2>

        <select
          bind:value={eqPreset}
          onchange={setEqualizer}
          class="w-full px-3 py-2.5 bg-[var(--color-bg-tertiary)] text-[var(--color-text-primary)] rounded-lg border border-[var(--color-border)] focus:outline-none focus:border-[var(--color-accent)]"
        >
          <option value="off">Off</option>
          <option value="bright">Bright</option>
          <option value="excited">Excited</option>
          <option value="mellow">Mellow</option>
          <option value="relaxed">Relaxed</option>
          <option value="vocal">Vocal</option>
          <option value="treble">Treble Boost</option>
          <option value="bass">Bass Boost</option>
          <option value="speech">Speech</option>
        </select>
      </section>
    {/if}

    <!-- DSEE (Audio Upsampling) -->
    {#if capabilities?.supports_dsee !== false}
      <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">DSEE Extreme</h2>
            <p class="text-xs text-[var(--color-text-muted)]">Audio upsampling</p>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              bind:checked={dseeEnabled}
              onchange={setDsee}
              class="sr-only peer"
            />
            <div class="w-11 h-6 bg-[var(--color-bg-tertiary)] peer-focus:ring-2 peer-focus:ring-[var(--color-accent)] rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-[var(--color-accent)]"></div>
          </label>
        </div>
      </section>
    {/if}

    <!-- Sound Position (XM3/XM4 only) -->
    {#if capabilities?.supports_sound_position !== false}
      <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
        <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Sound Position</h2>

        <select
          bind:value={soundPosition}
          onchange={setSoundPosition}
          class="w-full px-3 py-2.5 bg-[var(--color-bg-tertiary)] text-[var(--color-text-primary)] rounded-lg border border-[var(--color-border)] focus:outline-none focus:border-[var(--color-accent)]"
        >
          <option value="off">Off</option>
          <option value="front">Front</option>
          <option value="front_left">Front Left</option>
          <option value="front_right">Front Right</option>
          <option value="rear_left">Rear Left</option>
          <option value="rear_right">Rear Right</option>
        </select>
      </section>
    {/if}

    <!-- VPT Surround (XM3/XM4 only) -->
    {#if capabilities?.supports_vpt !== false}
      <section class="bg-[var(--color-bg-secondary)] rounded-xl p-4 space-y-4">
        <h2 class="text-sm font-medium text-[var(--color-text-secondary)]">Surround (VPT)</h2>

        <select
          bind:value={vptPreset}
          onchange={setVptPreset}
          class="w-full px-3 py-2.5 bg-[var(--color-bg-tertiary)] text-[var(--color-text-primary)] rounded-lg border border-[var(--color-border)] focus:outline-none focus:border-[var(--color-accent)]"
        >
          <option value="off">Off</option>
          <option value="outdoor">Outdoor Festival</option>
          <option value="arena">Arena</option>
          <option value="concert">Concert Hall</option>
          <option value="club">Club</option>
        </select>
      </section>
    {/if}
  {/if}

  <!-- Footer -->
  <footer class="text-center pt-4">
    <p class="text-xs text-[var(--color-text-muted)]">
      Not affiliated with Sony. Use at your own risk.
    </p>
  </footer>
</main>
