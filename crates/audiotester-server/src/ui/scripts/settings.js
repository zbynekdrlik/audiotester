// Settings page interactivity
(function () {
  "use strict";

  const deviceSelect = document.getElementById("device-select");
  const sampleRate = document.getElementById("sample-rate");
  const startBtn = document.getElementById("start-btn");
  const stopBtn = document.getElementById("stop-btn");
  const statusDisplay = document.getElementById("monitoring-status");
  const deviceInfo = document.getElementById("device-info");

  let devices = [];

  // Fetch device list
  async function loadDevices() {
    try {
      const resp = await fetch("/api/v1/devices");
      devices = await resp.json();

      deviceSelect.innerHTML = "";

      if (devices.length === 0) {
        const opt = document.createElement("option");
        opt.value = "";
        opt.textContent = "No devices found";
        deviceSelect.appendChild(opt);
        return;
      }

      devices.forEach(function (d) {
        const opt = document.createElement("option");
        opt.value = d.name;
        opt.textContent = d.name + (d.is_default ? " [DEFAULT]" : "");
        deviceSelect.appendChild(opt);
      });

      // Show info for first device
      showDeviceInfo(devices[0]);
    } catch (e) {
      deviceSelect.innerHTML =
        '<option value="">Error loading devices</option>';
    }
  }

  // Fetch current config
  async function loadConfig() {
    try {
      const resp = await fetch("/api/v1/config");
      const config = await resp.json();

      if (config.device) {
        deviceSelect.value = config.device;
      }
      sampleRate.value = config.sample_rate.toString();

      updateMonitoringUI(config.monitoring);
    } catch (e) {
      console.error("Failed to load config:", e);
    }
  }

  function showDeviceInfo(device) {
    if (!device) {
      deviceInfo.innerHTML = "<p>Select a device to see details.</p>";
      return;
    }
    deviceInfo.innerHTML =
      "<table>" +
      "<tr><td>Name</td><td>" +
      device.name +
      "</td></tr>" +
      "<tr><td>Input Ch</td><td>" +
      device.input_channels +
      "</td></tr>" +
      "<tr><td>Output Ch</td><td>" +
      device.output_channels +
      "</td></tr>" +
      "<tr><td>Sample Rates</td><td>" +
      (device.sample_rates.length > 0
        ? device.sample_rates.join(", ")
        : "N/A") +
      "</td></tr>" +
      "</table>";
  }

  function updateMonitoringUI(running) {
    if (running) {
      statusDisplay.textContent = "Running";
      statusDisplay.className = "status-display running";
      startBtn.disabled = true;
      stopBtn.disabled = false;
    } else {
      statusDisplay.textContent = "Stopped";
      statusDisplay.className = "status-display stopped";
      startBtn.disabled = false;
      stopBtn.disabled = true;
    }
  }

  // Event handlers
  deviceSelect.addEventListener("change", function () {
    const name = deviceSelect.value;
    const device = devices.find(function (d) {
      return d.name === name;
    });
    showDeviceInfo(device);

    // Update config
    fetch("/api/v1/config", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        device: name,
        sample_rate: parseInt(sampleRate.value),
      }),
    });
  });

  sampleRate.addEventListener("change", function () {
    fetch("/api/v1/config", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sample_rate: parseInt(sampleRate.value) }),
    });
  });

  startBtn.addEventListener("click", async function () {
    try {
      const resp = await fetch("/api/v1/monitoring", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled: true }),
      });
      const status = await resp.json();
      updateMonitoringUI(status.monitoring);
    } catch (e) {
      console.error("Failed to start:", e);
    }
  });

  stopBtn.addEventListener("click", async function () {
    try {
      const resp = await fetch("/api/v1/monitoring", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled: false }),
      });
      const status = await resp.json();
      updateMonitoringUI(status.monitoring);
    } catch (e) {
      console.error("Failed to stop:", e);
    }
  });

  // Initialize
  loadDevices().then(loadConfig);
})();
