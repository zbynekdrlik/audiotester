// Dashboard WebSocket + chart rendering
(function () {
  "use strict";

  const statusEl = document.getElementById("connection-status");

  // Summary bar elements
  const els = {
    latency: document.querySelector('[data-testid="latency-value"]'),
    min: document.querySelector('[data-testid="min-value"]'),
    max: document.querySelector('[data-testid="max-value"]'),
    avg: document.querySelector('[data-testid="avg-value"]'),
    lost: document.querySelector('[data-testid="lost-value"]'),
    corrupted: document.querySelector('[data-testid="corrupted-value"]'),
  };

  // Device info elements
  const deviceNameEl = document.getElementById("device-name");
  const sampleRateEl = document.getElementById("sample-rate-display");
  const uptimeEl = document.getElementById("uptime-display");
  const samplesSentEl = document.getElementById("samples-sent");
  const samplesReceivedEl = document.getElementById("samples-received");
  const resetBtn = document.getElementById("reset-btn");

  // Chart data
  let latencyData = [];
  let lossData = [];

  // Format uptime seconds into human-readable string
  function formatUptime(seconds) {
    if (!seconds || seconds === 0) return "--";
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h > 0) return h + "h " + m + "m";
    if (m > 0) return m + "m " + s + "s";
    return s + "s";
  }

  // Simple canvas chart renderer
  function drawChart(containerId, data, color, label) {
    const container = document.getElementById(containerId);
    if (!container || data.length === 0) return;

    let canvas = container.querySelector("canvas");
    if (!canvas) {
      canvas = document.createElement("canvas");
      container.appendChild(canvas);
    }

    const rect = container.getBoundingClientRect();
    canvas.width = rect.width;
    canvas.height = rect.height;

    const ctx = canvas.getContext("2d");
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const values = data.map((d) => d[1]);
    const minVal = Math.min(...values, 0);
    const maxVal = Math.max(...values, 1);
    const range = maxVal - minVal || 1;

    const pad = { top: 20, right: 10, bottom: 25, left: 50 };
    const w = canvas.width - pad.left - pad.right;
    const h = canvas.height - pad.top - pad.bottom;

    // Draw grid
    ctx.strokeStyle = "#2a2a4e";
    ctx.lineWidth = 1;
    for (let i = 0; i <= 4; i++) {
      const y = pad.top + (h * i) / 4;
      ctx.beginPath();
      ctx.moveTo(pad.left, y);
      ctx.lineTo(pad.left + w, y);
      ctx.stroke();

      // Y-axis labels
      const val = maxVal - (range * i) / 4;
      ctx.fillStyle = "#8892b0";
      ctx.font = "10px monospace";
      ctx.textAlign = "right";
      ctx.fillText(val.toFixed(1), pad.left - 5, y + 3);
    }

    // Draw line
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    for (let i = 0; i < data.length; i++) {
      const x = pad.left + (w * i) / (data.length - 1 || 1);
      const y = pad.top + h - ((values[i] - minVal) / range) * h;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Label
    ctx.fillStyle = color;
    ctx.font = "11px sans-serif";
    ctx.textAlign = "left";
    ctx.fillText(label, pad.left + 5, pad.top - 5);
  }

  function updateSummary(stats) {
    if (!stats) return;

    if (els.latency) {
      els.latency.textContent = stats.current_latency.toFixed(2);
      els.latency.className =
        "metric-value" +
        (stats.current_latency < 10
          ? " good"
          : stats.current_latency < 50
            ? " warning"
            : " error");
    }
    if (els.min)
      els.min.textContent =
        stats.min_latency > 0 ? stats.min_latency.toFixed(2) : "--";
    if (els.max)
      els.max.textContent =
        stats.max_latency > 0 ? stats.max_latency.toFixed(2) : "--";
    if (els.avg)
      els.avg.textContent =
        stats.avg_latency > 0 ? stats.avg_latency.toFixed(2) : "--";
    if (els.lost) els.lost.textContent = stats.total_lost.toString();
    if (els.corrupted)
      els.corrupted.textContent = stats.total_corrupted.toString();

    // Update device info
    if (deviceNameEl) {
      deviceNameEl.textContent = stats.device_name || "--";
    }
    if (sampleRateEl) {
      sampleRateEl.textContent =
        stats.sample_rate > 0 ? stats.sample_rate / 1000 + " kHz" : "--";
    }
    if (uptimeEl && stats.uptime_seconds !== undefined) {
      uptimeEl.textContent = formatUptime(stats.uptime_seconds);
    }
    // Update sample counters
    if (samplesSentEl && stats.samples_sent !== undefined) {
      samplesSentEl.textContent = formatSampleCount(stats.samples_sent);
    }
    if (samplesReceivedEl && stats.samples_received !== undefined) {
      samplesReceivedEl.textContent = formatSampleCount(stats.samples_received);
    }
  }

  // Format large sample counts (e.g., 1.2M, 450K)
  function formatSampleCount(count) {
    if (count >= 1000000000) return (count / 1000000000).toFixed(1) + "G";
    if (count >= 1000000) return (count / 1000000).toFixed(1) + "M";
    if (count >= 1000) return (count / 1000).toFixed(1) + "K";
    return count.toString();
  }

  function updateCharts(stats) {
    if (stats.latency_history && stats.latency_history.length > 0) {
      latencyData = stats.latency_history;
    }
    if (stats.loss_history && stats.loss_history.length > 0) {
      lossData = stats.loss_history;
    }

    drawChart("latency-chart", latencyData, "#00a0ff", "Latency (ms)");
    drawChart("loss-chart", lossData, "#ff4040", "Lost samples");
  }

  // Reset button handler
  if (resetBtn) {
    resetBtn.addEventListener("click", function () {
      fetch("/api/v1/reset", { method: "POST" })
        .then(function (res) {
          return res.json();
        })
        .then(function (data) {
          if (data.success) {
            // Counters will update on next WS message
            resetBtn.textContent = "Done!";
            setTimeout(function () {
              resetBtn.textContent = "Reset";
            }, 1500);
          }
        })
        .catch(function (err) {
          console.error("Reset failed:", err);
        });
    });
  }

  // WebSocket connection with auto-reconnect
  let ws = null;
  let reconnectTimer = null;

  function connect() {
    const proto = location.protocol === "https:" ? "wss:" : "ws:";
    ws = new WebSocket(proto + "//" + location.host + "/api/v1/ws");

    ws.onopen = function () {
      statusEl.textContent = "Connected";
      statusEl.className = "status-indicator connected";
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
    };

    ws.onmessage = function (event) {
      try {
        const stats = JSON.parse(event.data);
        updateSummary(stats);
        updateCharts(stats);
      } catch (e) {
        console.error("Failed to parse stats:", e);
      }
    };

    ws.onclose = function () {
      statusEl.textContent = "Disconnected";
      statusEl.className = "status-indicator error";
      reconnectTimer = setTimeout(connect, 2000);
    };

    ws.onerror = function () {
      ws.close();
    };
  }

  // Redraw charts on window resize
  window.addEventListener("resize", function () {
    drawChart("latency-chart", latencyData, "#00a0ff", "Latency (ms)");
    drawChart("loss-chart", lossData, "#ff4040", "Lost samples");
  });

  connect();
})();
