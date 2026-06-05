// ── Dashboard ───────────────────────────────────────────────────────────────
// Chart.js powered metrics dashboard

let chartEventRate = null;
let chartEventTypes = null;
let chartWatchRoots = null;
let chartNotifications = null;
let dashboardRefreshTimer = null;
const DASHBOARD_REFRESH_INTERVAL = 30000; // 30 seconds

// Chart.js global defaults
function initChartDefaults() {
  if (typeof Chart === 'undefined') return;

  const style = getComputedStyle(document.documentElement);
  const textColor = style.getPropertyValue('--text-muted').trim() || '#8a90a0';
  const gridColor = style.getPropertyValue('--border').trim() || 'rgba(255,255,255,0.06)';

  Chart.defaults.color = textColor;
  Chart.defaults.borderColor = gridColor;
  Chart.defaults.font.family = "'IBM Plex Mono', monospace";
  Chart.defaults.font.size = 11;
  Chart.defaults.plugins.legend.display = false;
  Chart.defaults.animation.duration = 300;
}

// Color palette for charts
const CHART_COLORS = {
  amber: 'rgba(240, 160, 48, 0.8)',
  amberFill: 'rgba(240, 160, 48, 0.1)',
  blue: 'rgba(96, 165, 250, 0.8)',
  blueFill: 'rgba(96, 165, 250, 0.1)',
  green: 'rgba(74, 222, 128, 0.8)',
  greenFill: 'rgba(74, 222, 128, 0.1)',
  red: 'rgba(248, 113, 113, 0.8)',
  redFill: 'rgba(248, 113, 113, 0.1)',
  purple: 'rgba(192, 132, 252, 0.8)',
  purpleFill: 'rgba(192, 132, 252, 0.1)',
  cyan: 'rgba(34, 211, 238, 0.8)',
  cyanFill: 'rgba(34, 211, 238, 0.1)',
};

const TYPE_COLORS = [
  CHART_COLORS.amber,
  CHART_COLORS.blue,
  CHART_COLORS.green,
  CHART_COLORS.red,
  CHART_COLORS.purple,
  CHART_COLORS.cyan,
];

// Format uptime seconds to human readable
function formatUptime(seconds) {
  if (seconds < 60) return seconds + 's';
  if (seconds < 3600) return Math.floor(seconds / 60) + 'm';
  if (seconds < 86400) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    return h + 'h ' + m + 'm';
  }
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  return d + 'd ' + h + 'h';
}

// Format bytes to human readable
function formatBytes(bytes) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

// Format number with commas
function formatNumber(num) {
  return num.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

// Create or update the event rate line chart
function updateEventRateChart(data, range) {
  if (typeof Chart === 'undefined') return;

  const ctx = document.getElementById('chart-event-rate');
  if (!ctx) return;

  // Format labels based on range
  const labels = data.map(p => {
    const d = new Date(p.timestamp);
    if (range === '7d') {
      // For 7d: show date + hour
      return (d.getMonth() + 1) + '/' + d.getDate() + ' ' +
             d.getHours().toString().padStart(2, '0') + ':00';
    } else {
      // For 1h: show hour:minute
      return d.getHours().toString().padStart(2, '0') + ':' +
             d.getMinutes().toString().padStart(2, '0');
    }
  });
  const values = data.map(p => p.value);

  if (chartEventRate) {
    chartEventRate.data.labels = labels;
    chartEventRate.data.datasets[0].data = values;
    chartEventRate.update('none');
  } else {
    chartEventRate = new Chart(ctx, {
      type: 'line',
      data: {
        labels,
        datasets: [{
          data: values,
          borderColor: CHART_COLORS.amber,
          backgroundColor: CHART_COLORS.amberFill,
          borderWidth: 2,
          fill: true,
          tension: 0.4,
          pointRadius: 0,
          pointHitRadius: 10,
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        interaction: {
          intersect: false,
          mode: 'index'
        },
        scales: {
          x: {
            grid: { display: false },
            ticks: { maxTicksLimit: 10 }
          },
          y: {
            beginAtZero: true,
            grid: { color: 'rgba(255,255,255,0.03)' },
            ticks: { stepSize: 1 }
          }
        },
        plugins: {
          tooltip: {
            backgroundColor: 'rgba(6, 10, 20, 0.9)',
            titleColor: '#f0a030',
            bodyColor: '#c8d0e0',
            borderColor: 'rgba(240, 160, 48, 0.3)',
            borderWidth: 1,
            padding: 10,
            displayColors: false,
            callbacks: {
              label: (ctx) => ctx.parsed.y + ' 事件'
            }
          }
        }
      }
    });
  }
}

// Create or update the event types doughnut chart
function updateEventTypesChart(data) {
  if (typeof Chart === 'undefined') return;

  const ctx = document.getElementById('chart-event-types');
  if (!ctx) return;

  const labels = data.map(d => d.event_type);
  const values = data.map(d => d.count);
  const colors = data.map((_, i) => TYPE_COLORS[i % TYPE_COLORS.length]);

  // Update subtitle
  const total = values.reduce((a, b) => a + b, 0);
  const subtitle = document.getElementById('chart-type-subtitle');
  if (subtitle) subtitle.textContent = formatNumber(total) + ' 总计';

  if (chartEventTypes) {
    chartEventTypes.data.labels = labels;
    chartEventTypes.data.datasets[0].data = values;
    chartEventTypes.data.datasets[0].backgroundColor = colors;
    chartEventTypes.update('none');
  } else {
    chartEventTypes = new Chart(ctx, {
      type: 'doughnut',
      data: {
        labels,
        datasets: [{
          data: values,
          backgroundColor: colors,
          borderWidth: 0,
          hoverOffset: 4
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        cutout: '65%',
        plugins: {
          legend: {
            display: true,
            position: 'right',
            labels: {
              padding: 12,
              usePointStyle: true,
              pointStyleWidth: 8,
              font: { size: 11 }
            }
          },
          tooltip: {
            backgroundColor: 'rgba(6, 10, 20, 0.9)',
            titleColor: '#f0a030',
            bodyColor: '#c8d0e0',
            borderColor: 'rgba(240, 160, 48, 0.3)',
            borderWidth: 1,
            callbacks: {
              label: (ctx) => {
                const pct = total > 0 ? ((ctx.parsed / total) * 100).toFixed(1) : 0;
                return ctx.label + ': ' + ctx.parsed + ' (' + pct + '%)';
              }
            }
          }
        }
      }
    });
  }
}

// Create or update the watch roots stacked bar chart
function updateWatchRootsChart(rootsData, typeRootData) {
  if (typeof Chart === 'undefined') return;

  const ctx = document.getElementById('chart-watch-roots');
  if (!ctx) return;

  // Cache data for legend toggle
  window._rootsData = rootsData;
  window._typeRootData = typeRootData;

  // Get visible event types from chart legend
  const visibleTypes = getVisibleEventTypes();

  // Filter typeRootData by visible types
  const filteredTypeRoot = typeRootData.filter(d => visibleTypes.includes(d.event_type));

  // Recompute top roots based on filtered data
  const rootCounts = {};
  filteredTypeRoot.forEach(d => {
    rootCounts[d.root] = (rootCounts[d.root] || 0) + d.count;
  });

  const topRoots = Object.entries(rootCounts)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 5)
    .map(([root]) => root);

  // Shorten path for display
  const labels = topRoots.map(root => {
    const parts = root.split('/');
    return parts.length > 3 ? '.../' + parts.slice(-2).join('/') : root;
  });

  // Get all unique event types for legend
  const allEventTypes = [...new Set(typeRootData.map(d => d.event_type))];

  // Build datasets for each event type
  const datasets = allEventTypes.map((type, i) => {
    const isVisible = visibleTypes.includes(type);
    const data = topRoots.map(root => {
      const match = typeRootData.find(d => d.event_type === type && d.root === root);
      return match ? match.count : 0;
    });
    return {
      label: type,
      data,
      backgroundColor: isVisible ? TYPE_COLORS[i % TYPE_COLORS.length] : 'transparent',
      borderWidth: 0,
      borderRadius: 2,
      hidden: !isVisible,
    };
  });

  if (chartWatchRoots) {
    chartWatchRoots.data.labels = labels;
    chartWatchRoots.data.datasets = datasets;
    chartWatchRoots.update('none');
  } else {
    chartWatchRoots = new Chart(ctx, {
      type: 'bar',
      data: { labels, datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        indexAxis: 'y',
        scales: {
          x: {
            stacked: true,
            beginAtZero: true,
            grid: { color: 'rgba(255,255,255,0.03)' }
          },
          y: {
            stacked: true,
            grid: { display: false }
          }
        },
        plugins: {
          legend: {
            display: true,
            position: 'top',
            labels: {
              padding: 12,
              usePointStyle: true,
              pointStyleWidth: 8,
              font: { size: 10 }
            },
            onClick: (e, legendItem, legend) => {
              // Toggle visibility
              const index = legendItem.datasetIndex;
              const ci = legend.chart;
              const meta = ci.getDatasetMeta(index);
              meta.hidden = meta.hidden === null ? !ci.data.datasets[index].hidden : null;
              ci.update();

              // Recompute chart with new visible types
              updateWatchRootsChart(window._rootsData, window._typeRootData);
            }
          },
          tooltip: {
            backgroundColor: 'rgba(6, 10, 20, 0.9)',
            titleColor: '#f0a030',
            bodyColor: '#c8d0e0',
            borderColor: 'rgba(240, 160, 48, 0.3)',
            borderWidth: 1,
            callbacks: {
              label: (ctx) => ctx.dataset.label + ': ' + ctx.parsed.x + ' 事件'
            }
          }
        }
      }
    });
  }
}

// Get visible event types from chart legend
function getVisibleEventTypes() {
  if (!chartWatchRoots) {
    return EVENT_TYPES;
  }

  const visible = [];
  chartWatchRoots.data.datasets.forEach((ds, i) => {
    const meta = chartWatchRoots.getDatasetMeta(i);
    if (meta.hidden === null ? !ds.hidden : !meta.hidden) {
      visible.push(ds.label);
    }
  });
  return visible;
}

// Create or update the notifications chart
function updateNotificationsChart(sent, failed) {
  if (typeof Chart === 'undefined') return;

  const ctx = document.getElementById('chart-notifications');
  if (!ctx) return;

  // Merge sent and failed data by type
  const types = new Set();
  sent.forEach(d => types.add(d.event_type));
  failed.forEach(d => types.add(d.event_type));

  const labels = Array.from(types);
  const sentValues = labels.map(t => {
    const found = sent.find(d => d.event_type === t);
    return found ? found.count : 0;
  });
  const failedValues = labels.map(t => {
    const found = failed.find(d => d.event_type === t);
    return found ? found.count : 0;
  });

  if (chartNotifications) {
    chartNotifications.data.labels = labels;
    chartNotifications.data.datasets[0].data = sentValues;
    chartNotifications.data.datasets[1].data = failedValues;
    chartNotifications.update('none');
  } else {
    chartNotifications = new Chart(ctx, {
      type: 'bar',
      data: {
        labels,
        datasets: [
          {
            label: '成功',
            data: sentValues,
            backgroundColor: CHART_COLORS.green,
            borderWidth: 0,
            borderRadius: 4
          },
          {
            label: '失败',
            data: failedValues,
            backgroundColor: CHART_COLORS.red,
            borderWidth: 0,
            borderRadius: 4
          }
        ]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            grid: { display: false }
          },
          y: {
            beginAtZero: true,
            grid: { color: 'rgba(255,255,255,0.03)' },
            ticks: { stepSize: 1 }
          }
        },
        plugins: {
          legend: {
            display: true,
            position: 'top',
            labels: {
              padding: 16,
              usePointStyle: true,
              pointStyleWidth: 8
            }
          },
          tooltip: {
            backgroundColor: 'rgba(6, 10, 20, 0.9)',
            titleColor: '#f0a030',
            bodyColor: '#c8d0e0',
            borderColor: 'rgba(240, 160, 48, 0.3)',
            borderWidth: 1
          }
        }
      }
    });
  }
}

// Update status card values
function updateStatusCards(system) {
  const eventsEl = document.getElementById('metric-events-total');
  const uptimeEl = document.getElementById('metric-uptime');
  const watchersEl = document.getElementById('metric-watchers');
  const dbSizeEl = document.getElementById('metric-db-size');
  const batchesEl = document.getElementById('metric-batches');
  const droppedEl = document.getElementById('metric-dropped');
  const dedupedEl = document.getElementById('metric-deduped');
  const queueEl = document.getElementById('metric-queue');

  if (eventsEl) eventsEl.textContent = formatNumber(system.events_total || 0);
  if (uptimeEl) uptimeEl.textContent = formatUptime(system.uptime_seconds || 0);
  if (watchersEl) watchersEl.textContent = system.active_watchers || 0;
  if (dbSizeEl) dbSizeEl.textContent = formatBytes(system.db_size_bytes || 0);
  if (batchesEl) batchesEl.textContent = formatNumber(system.batches_flushed || 0);
  if (droppedEl) droppedEl.textContent = formatNumber(system.events_dropped || 0);
  if (dedupedEl) dedupedEl.textContent = formatNumber(system.events_deduped || 0);
  if (queueEl) queueEl.textContent = system.queue_depth || 0;
}

// Fetch and update all dashboard data
async function refreshDashboard() {
  try {
    const token = localStorage.getItem('dm_token');
    const headers = token ? { 'Authorization': 'Bearer ' + token } : {};

    const resp = await fetch('/api/metrics/chart', { headers });
    if (!resp.ok) {
      if (resp.status === 401) {
        // Token expired, redirect to login
        showLogin();
        return;
      }
      throw new Error('HTTP ' + resp.status);
    }

    const data = await resp.json();

    // Cache data for time range switching
    window._chartData = data;

    // Update charts with current time range
    const rateData = currentTimeRange === '1h' ? data.event_rate_1h : data.event_rate_7d;
    if (rateData) {
      updateEventRateChart(rateData, currentTimeRange);
    }

    if (data.events_by_type) {
      updateEventTypesChart(data.events_by_type);
    }

    // Update watch roots chart with stacked bar (type + root)
    if (data.events_by_root && data.events_by_type_root) {
      updateWatchRootsChart(data.events_by_root, data.events_by_type_root);
    }

    if (data.notifications) {
      updateNotificationsChart(
        data.notifications.sent || [],
        data.notifications.failed || []
      );
    }

    // Update status cards
    if (data.system) {
      data.system.events_total = data.events_total;
      updateStatusCards(data.system);
    }

  } catch (err) {
    console.error('Dashboard refresh failed:', err);
  }
}

// Current time range for event rate chart
let currentTimeRange = '1h';

// Switch between 1h and 7d time ranges
function switchTimeRange(range) {
  currentTimeRange = range;

  // Update button states
  document.querySelectorAll('.chart-toggle-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.range === range);
  });

  // Update chart with cached data
  if (window._chartData) {
    const data = range === '1h' ? window._chartData.event_rate_1h : window._chartData.event_rate_7d;
    updateEventRateChart(data, range);
  }
}

// Initialize dashboard
function initDashboard() {
  initChartDefaults();
  refreshDashboard();

  // Auto-refresh
  if (dashboardRefreshTimer) clearInterval(dashboardRefreshTimer);
  dashboardRefreshTimer = setInterval(refreshDashboard, DASHBOARD_REFRESH_INTERVAL);
}

// Stop dashboard refresh
function stopDashboard() {
  if (dashboardRefreshTimer) {
    clearInterval(dashboardRefreshTimer);
    dashboardRefreshTimer = null;
  }
}

// Destroy all charts (for cleanup)
function destroyDashboard() {
  stopDashboard();
  if (chartEventRate) { chartEventRate.destroy(); chartEventRate = null; }
  if (chartEventTypes) { chartEventTypes.destroy(); chartEventTypes = null; }
  if (chartWatchRoots) { chartWatchRoots.destroy(); chartWatchRoots = null; }
  if (chartNotifications) { chartNotifications.destroy(); chartNotifications = null; }
}
