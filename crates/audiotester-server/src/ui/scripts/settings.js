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
      deviceInfo.innerHTML = "";
      var p = document.createElement("p");
      p.textContent = "Select a device to see details.";
      deviceInfo.appendChild(p);
      return;
    }
    var table = document.createElement("table");
    [
      ["Name", device.name],
      ["Input Ch", device.input_channels],
      ["Output Ch", device.output_channels],
      [
        "Sample Rates",
        device.sample_rates.length > 0 ? device.sample_rates.join(", ") : "N/A",
      ],
    ].forEach(function (row) {
      var tr = document.createElement("tr");
      var td1 = document.createElement("td");
      var td2 = document.createElement("td");
      td1.textContent = row[0];
      td2.textContent = String(row[1]);
      tr.appendChild(td1);
      tr.appendChild(td2);
      table.appendChild(tr);
    });
    deviceInfo.innerHTML = "";
    deviceInfo.appendChild(table);
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
  deviceSelect.addEventListener("change", async function () {
    const name = deviceSelect.value;
    const device = devices.find(function (d) {
      return d.name === name;
    });
    showDeviceInfo(device);

    try {
      const resp = await fetch("/api/v1/config", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          device: name,
          sample_rate: parseInt(sampleRate.value),
        }),
      });
      if (!resp.ok) throw new Error("Config update failed: " + resp.status);
    } catch (e) {
      console.error("Failed to update device:", e);
      loadConfig();
    }
  });

  sampleRate.addEventListener("change", async function () {
    try {
      const resp = await fetch("/api/v1/config", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ sample_rate: parseInt(sampleRate.value) }),
      });
      if (!resp.ok) throw new Error("Config update failed: " + resp.status);
    } catch (e) {
      console.error("Failed to update sample rate:", e);
      loadConfig();
    }
  });

  function showError(message) {
    var existing = document.querySelector(".error-notification");
    if (existing) existing.remove();

    var el = document.createElement("div");
    el.className = "error-notification";
    el.textContent = message;
    document.body.appendChild(el);

    setTimeout(function () {
      if (el.parentNode) el.remove();
    }, 5000);
  }

  startBtn.addEventListener("click", async function () {
    try {
      const resp = await fetch("/api/v1/monitoring", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled: true }),
      });
      if (!resp.ok) {
        const errText = await resp.text();
        showError("Start failed: " + errText);
        return;
      }
      const status = await resp.json();
      updateMonitoringUI(status.monitoring);
    } catch (e) {
      showError("Start failed: " + e.message);
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
      if (!resp.ok) {
        const errText = await resp.text();
        showError("Stop failed: " + errText);
        return;
      }
      const status = await resp.json();
      updateMonitoringUI(status.monitoring);
    } catch (e) {
      showError("Stop failed: " + e.message);
      console.error("Failed to stop:", e);
    }
  });

  // Initialize
  loadDevices().then(loadConfig);
})();
