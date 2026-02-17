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
  const signalStatusEl = document.getElementById("signal-status");
  const remoteUrlEl = document.getElementById("remote-url");
  const resetBtn = document.getElementById("reset-btn");

  // Chart data
  let latencyData = [];

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

  // Simple canvas chart renderer (used for latency)
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
    if (els.lost) {
      if (stats.counter_silent && stats.estimated_loss > 0) {
        // During silence: show combined total with ~ prefix
        var combined = stats.total_lost + stats.estimated_loss;
        els.lost.textContent = "~" + combined.toString();
        els.lost.className = "metric-value warning";
      } else if (stats.total_lost > 0) {
        els.lost.textContent = stats.total_lost.toString();
        els.lost.className = "metric-value error";
      } else {
        els.lost.textContent = stats.total_lost.toString();
        els.lost.className = "metric-value";
      }
    }
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
    // Update signal status
    if (signalStatusEl) {
      if (stats.signal_lost) {
        signalStatusEl.textContent = "NO SIGNAL";
        signalStatusEl.classList.add("warning");
        signalStatusEl.classList.remove("ok");
      } else {
        signalStatusEl.textContent = "Signal OK";
        signalStatusEl.classList.add("ok");
        signalStatusEl.classList.remove("warning");
      }
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

    drawChart("latency-chart", latencyData, "#00a0ff", "Latency (ms)");
  }

  // ─── Loss Timeline (Lightweight Charts) ───────────────────────────

  // Convert UTC unix timestamp to local time for chart display.
  // Lightweight Charts treats all timestamps as UTC, so we shift by
  // the local timezone offset to show correct local times on the axis.
  function timeToLocal(utcTimestamp) {
    var d = new Date(utcTimestamp * 1000);
    return (
      Date.UTC(
        d.getFullYear(),
        d.getMonth(),
        d.getDate(),
        d.getHours(),
        d.getMinutes(),
        d.getSeconds(),
      ) / 1000
    );
  }

  var lossChart = null;
  var lossHistogram = null;
  var lossMarkers = null;
  var lossTimelineRange = "24h";
  var lossRefreshTimer = null;
  var lastTotalLost = 0;

  function initLossTimeline() {
    var container = document.getElementById("loss-timeline");
    if (!container || !window.LightweightCharts) return;

    lossChart = LightweightCharts.createChart(container, {
      layout: {
        background: { color: "#0f1729" },
        textColor: "#8892b0",
      },
      grid: {
        vertLines: { color: "#2a2a4e" },
        horzLines: { color: "#2a2a4e" },
      },
      timeScale: {
        timeVisible: true,
        secondsVisible: false,
        borderColor: "#2a2a4e",
        rightOffset: 3,
      },
      rightPriceScale: {
        borderColor: "#2a2a4e",
        mode: 1, // Logarithmic scale — large spikes don't squash small losses
      },
      crosshair: {
        mode: 0,
      },
    });

    lossHistogram = lossChart.addSeries(LightweightCharts.HistogramSeries, {
      color: "#ff4040",
      priceFormat: { type: "volume" },
    });

    // Handle resize
    new ResizeObserver(function () {
      lossChart.applyOptions({
        width: container.clientWidth,
        height: container.clientHeight,
      });
    }).observe(container);

    // Setup zoom button handlers
    var zoomControls = document.getElementById("loss-zoom-controls");
    if (zoomControls) {
      zoomControls.addEventListener("click", function (e) {
        var btn = e.target.closest(".zoom-btn");
        if (!btn) return;
        var range = btn.getAttribute("data-range");
        if (!range) return;

        // Update active state
        var buttons = zoomControls.querySelectorAll(".zoom-btn");
        for (var i = 0; i < buttons.length; i++) {
          buttons[i].classList.remove("active");
        }
        btn.classList.add("active");

        // Fetch new range
        lossTimelineRange = range;
        fetchLossTimeline(range);
      });
    }

    // Initial fetch
    fetchLossTimeline(lossTimelineRange);

    // Refresh every 30 seconds
    lossRefreshTimer = setInterval(function () {
      fetchLossTimeline(lossTimelineRange);
    }, 30000);
  }

  function fetchLossTimeline(range) {
    fetch("/api/v1/loss-timeline?range=" + range)
      .then(function (r) {
        return r.json();
      })
      .then(function (data) {
        if (!lossHistogram || !data.buckets) return;
        var bucketSize = data.bucket_size_secs || 300;
        var chartData = data.buckets.map(function (b) {
          return {
            time: timeToLocal(b.t),
            value: b.loss,
            color:
              b.loss > 1000
                ? "#ff4040"
                : b.loss > 0
                  ? "#ff8040"
                  : "transparent",
          };
        });

        // Ensure data extends to "now" so the right edge represents current time
        var nowUtc = Math.floor(Date.now() / 1000);
        var nowAligned = nowUtc - (nowUtc % bucketSize);
        var nowLocal = timeToLocal(nowAligned);
        if (
          chartData.length > 0 &&
          chartData[chartData.length - 1].time < nowLocal
        ) {
          chartData.push({ time: nowLocal, value: 0, color: "transparent" });
        }

        lossHistogram.setData(chartData);

        // Add "Now" marker at the current time position
        var markerTime =
          chartData.length > 0
            ? chartData[chartData.length - 1].time
            : nowLocal;
        var markerDef = [
          {
            time: markerTime,
            position: "aboveBar",
            color: "#00d4ff",
            shape: "arrowDown",
            text: "Now",
          },
        ];
        if (lossMarkers) {
          lossMarkers.setMarkers(markerDef);
        } else if (LightweightCharts.createSeriesMarkers) {
          lossMarkers = LightweightCharts.createSeriesMarkers(
            lossHistogram,
            markerDef,
          );
        }
      })
      .catch(function (err) {
        console.error("Failed to fetch loss timeline:", err);
      });
  }

  // ─── Reset button handler ─────────────────────────────────────────

  if (resetBtn) {
    resetBtn.addEventListener("click", function () {
      fetch("/api/v1/reset", { method: "POST" })
        .then(function (res) {
          if (res.ok) {
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

  // ─── WebSocket connection with auto-reconnect ─────────────────────

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

        // Trigger timeline refresh if loss changed
        if (stats.total_lost !== lastTotalLost) {
          lastTotalLost = stats.total_lost;
          if (lossHistogram) {
            fetchLossTimeline(lossTimelineRange);
          }
        }
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

  // Redraw latency chart on window resize
  window.addEventListener("resize", function () {
    drawChart("latency-chart", latencyData, "#00a0ff", "Latency (ms)");
  });

  // Fetch and display remote URL
  function loadRemoteUrl() {
    if (!remoteUrlEl) return;
    fetch("/api/v1/remote-url")
      .then(function (resp) {
        return resp.json();
      })
      .then(function (data) {
        if (data.url) {
          remoteUrlEl.textContent = data.url;
          remoteUrlEl.style.cursor = "pointer";
          remoteUrlEl.onclick = function () {
            navigator.clipboard
              .writeText(data.url)
              .then(function () {
                var originalText = remoteUrlEl.textContent;
                remoteUrlEl.textContent = "Copied!";
                setTimeout(function () {
                  remoteUrlEl.textContent = originalText;
                }, 1500);
              })
              .catch(function (err) {
                console.error("Failed to copy:", err);
              });
          };
        }
      })
      .catch(function (err) {
        console.error("Failed to load remote URL:", err);
      });
  }

  // Fetch and display version info
  function loadVersionInfo() {
    var versionEl = document.getElementById("version-info");
    if (!versionEl) return;
    fetch("/api/v1/status")
      .then(function (resp) {
        return resp.json();
      })
      .then(function (data) {
        if (data.version && data.build_date) {
          versionEl.textContent =
            "v" + data.version + " (" + data.build_date + ")";
        }
      })
      .catch(function (err) {
        console.error("Failed to load version info:", err);
      });
  }

  loadVersionInfo();
  loadRemoteUrl();
  initLossTimeline();
  connect();
})();
