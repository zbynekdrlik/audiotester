// Dashboard WebSocket + chart rendering
(function () {
  "use strict";

  const statusEl = document.getElementById("connection-status");

  // Summary bar elements
  const els = {
    latency: document.querySelector('[data-testid="latency-value"]'),
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

  function updateSummary(stats) {
    if (!stats) return;

    if (els.latency) {
      els.latency.textContent = stats.current_latency.toFixed(1);
      els.latency.className =
        "metric-value" +
        (stats.current_latency < 10
          ? " good"
          : stats.current_latency < 50
            ? " warning"
            : " error");
    }
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

  // ─── Shared: UTC to local timestamp conversion ─────────────────────

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

  // ─── Loss Timeline (Lightweight Charts) ───────────────────────────

  var lossChart = null;
  var lossHistogram = null;
  var lossMarkers = null;
  var lossTimelineRange = "1h";
  var lossRefreshTimer = null;
  var lastTotalLost = 0;
  var lossLiveMode = true;
  var lossUpdating = false;

  function initLossTimeline() {
    var container = document.getElementById("loss-timeline");
    if (!container || !window.LightweightCharts) return;

    lossChart = LightweightCharts.createChart(container, {
      layout: {
        background: { color: "#0f1729" },
        textColor: "#8892b0",
        attributionLogo: false,
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
        mode: 0, // Normal scale with sqrt-transformed values for balanced visual range
      },
      crosshair: {
        mode: 0,
      },
    });

    lossHistogram = lossChart.addSeries(LightweightCharts.HistogramSeries, {
      color: "#ff4040",
      priceFormat: {
        type: "custom",
        formatter: function (price) {
          var original = Math.round(price * price);
          if (original >= 1000000) return (original / 1000000).toFixed(1) + "M";
          if (original >= 1000) return (original / 1000).toFixed(1) + "K";
          return original.toString();
        },
        minMove: 0.01,
      },
    });

    // Tooltip overlay for exact loss values on hover
    var toolTip = document.createElement("div");
    toolTip.className = "loss-tooltip";
    toolTip.style.display = "none";
    container.style.position = "relative";
    container.appendChild(toolTip);

    lossChart.subscribeCrosshairMove(function (param) {
      if (
        !param.point ||
        !param.time ||
        param.point.x < 0 ||
        param.point.y < 0
      ) {
        toolTip.style.display = "none";
        return;
      }
      var data = param.seriesData.get(lossHistogram);
      if (!data || !data.value) {
        toolTip.style.display = "none";
        return;
      }
      var original = Math.round(data.value * data.value);
      var formatted;
      if (original >= 1000000)
        formatted = (original / 1000000).toFixed(1) + "M";
      else if (original >= 1000) formatted = (original / 1000).toFixed(1) + "K";
      else formatted = original.toLocaleString();

      var d = new Date(param.time * 1000);
      var timeStr = d.toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });

      toolTip.innerHTML =
        '<div class="loss-tooltip-value">' +
        formatted +
        " lost</div>" +
        '<div class="loss-tooltip-time">' +
        timeStr +
        "</div>";
      toolTip.style.display = "block";

      var chartRect = container.getBoundingClientRect();
      var x = Math.max(0, Math.min(param.point.x - 40, chartRect.width - 90));
      toolTip.style.left = x + "px";
      toolTip.style.top = "8px";
    });

    // Handle resize
    new ResizeObserver(function () {
      lossChart.applyOptions({
        width: container.clientWidth,
        height: container.clientHeight,
      });
    }).observe(container);

    // Detect manual panning to disable live mode
    lossChart.timeScale().subscribeVisibleLogicalRangeChange(function () {
      if (!lossUpdating && lossLiveMode) {
        lossLiveMode = false;
        var btn = document.getElementById("loss-live-btn");
        if (btn) btn.classList.remove("active");
      }
    });

    // Live button handler
    var lossLiveBtn = document.getElementById("loss-live-btn");
    if (lossLiveBtn) {
      lossLiveBtn.addEventListener("click", function () {
        lossLiveMode = true;
        lossLiveBtn.classList.add("active");
        if (lossChart) {
          lossChart.timeScale().scrollToRealTime();
        }
      });
    }

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
            value: b.loss > 0 ? Math.sqrt(b.loss) : 0,
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

        lossUpdating = true;
        lossHistogram.setData(chartData);
        if (lossLiveMode) {
          lossChart.timeScale().scrollToRealTime();
        }
        setTimeout(function () {
          lossUpdating = false;
        }, 0);

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

  // ─── Latency Timeline (Lightweight Charts) ────────────────────────

  var latencyChart = null;
  var latencyLine = null;
  var latencyMarkers = null;
  var latencyTimelineRange = "1h";
  var latencyRefreshTimer = null;
  var latencyLiveMode = true;
  var latencyUpdating = false;

  function initLatencyTimeline() {
    var container = document.getElementById("latency-chart");
    if (!container || !window.LightweightCharts) return;

    latencyChart = LightweightCharts.createChart(container, {
      layout: {
        background: { color: "#0f1729" },
        textColor: "#8892b0",
        attributionLogo: false,
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
      },
      crosshair: {
        mode: 0,
      },
    });

    latencyLine = latencyChart.addSeries(LightweightCharts.LineSeries, {
      color: "#00a0ff",
      lineWidth: 2,
      priceFormat: {
        type: "custom",
        formatter: function (price) {
          return price.toFixed(1) + " ms";
        },
        minMove: 0.1,
      },
      autoscaleInfoProvider: function () {
        return {
          priceRange: { minValue: null, maxValue: null },
          margins: { above: 0.1, below: 0.1 },
        };
      },
    });

    // Tooltip overlay for exact latency values on hover
    var toolTip = document.createElement("div");
    toolTip.className = "latency-tooltip";
    toolTip.style.display = "none";
    container.style.position = "relative";
    container.appendChild(toolTip);

    latencyChart.subscribeCrosshairMove(function (param) {
      if (
        !param.point ||
        !param.time ||
        param.point.x < 0 ||
        param.point.y < 0
      ) {
        toolTip.style.display = "none";
        return;
      }
      var data = param.seriesData.get(latencyLine);
      if (!data || data.value === undefined) {
        toolTip.style.display = "none";
        return;
      }

      var d = new Date(param.time * 1000);
      var timeStr = d.toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });

      toolTip.innerHTML =
        '<div class="latency-tooltip-value">' +
        data.value.toFixed(1) +
        " ms</div>" +
        '<div class="latency-tooltip-time">' +
        timeStr +
        "</div>";
      toolTip.style.display = "block";

      var chartRect = container.getBoundingClientRect();
      var x = Math.max(0, Math.min(param.point.x - 40, chartRect.width - 90));
      toolTip.style.left = x + "px";
      toolTip.style.top = "8px";
    });

    // Handle resize
    new ResizeObserver(function () {
      latencyChart.applyOptions({
        width: container.clientWidth,
        height: container.clientHeight,
      });
    }).observe(container);

    // Detect manual panning to disable live mode
    latencyChart.timeScale().subscribeVisibleLogicalRangeChange(function () {
      if (!latencyUpdating && latencyLiveMode) {
        latencyLiveMode = false;
        var btn = document.getElementById("latency-live-btn");
        if (btn) btn.classList.remove("active");
      }
    });

    // Live button handler
    var latencyLiveBtn = document.getElementById("latency-live-btn");
    if (latencyLiveBtn) {
      latencyLiveBtn.addEventListener("click", function () {
        latencyLiveMode = true;
        latencyLiveBtn.classList.add("active");
        if (latencyChart) {
          latencyChart.timeScale().scrollToRealTime();
        }
      });
    }

    // Setup zoom button handlers
    var zoomControls = document.getElementById("latency-zoom-controls");
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
        latencyTimelineRange = range;
        fetchLatencyTimeline(range);
      });
    }

    // Initial fetch
    fetchLatencyTimeline(latencyTimelineRange);

    // Refresh every 30 seconds
    latencyRefreshTimer = setInterval(function () {
      fetchLatencyTimeline(latencyTimelineRange);
    }, 30000);
  }

  function fetchLatencyTimeline(range) {
    fetch("/api/v1/latency-timeline?range=" + range)
      .then(function (r) {
        return r.json();
      })
      .then(function (data) {
        if (!latencyLine || !data.buckets) return;
        var chartData = data.buckets
          .filter(function (b) {
            return b.avg > 0;
          })
          .map(function (b) {
            return {
              time: timeToLocal(b.t),
              value: Math.round(b.avg * 10) / 10,
            };
          });

        latencyUpdating = true;
        latencyLine.setData(chartData);
        if (latencyLiveMode) {
          latencyChart.timeScale().scrollToRealTime();
        }
        setTimeout(function () {
          latencyUpdating = false;
        }, 0);

        // Add "Now" marker on the last data point
        if (chartData.length > 0) {
          var markerTime = chartData[chartData.length - 1].time;
          var markerDef = [
            {
              time: markerTime,
              position: "aboveBar",
              color: "#00d4ff",
              shape: "arrowDown",
              text: "Now",
            },
          ];
          if (latencyMarkers) {
            latencyMarkers.setMarkers(markerDef);
          } else if (LightweightCharts.createSeriesMarkers) {
            latencyMarkers = LightweightCharts.createSeriesMarkers(
              latencyLine,
              markerDef,
            );
          }
        }
      })
      .catch(function (err) {
        console.error("Failed to fetch latency timeline:", err);
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

        // Trigger loss timeline refresh if loss changed
        if (stats.total_lost !== lastTotalLost) {
          lastTotalLost = stats.total_lost;
          if (lossHistogram) {
            fetchLossTimeline(lossTimelineRange);
          }
        }

        // Trigger latency timeline refresh on new data
        if (latencyLine && stats.measurement_count > 0) {
          fetchLatencyTimeline(latencyTimelineRange);
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
  initLatencyTimeline();
  connect();
})();
