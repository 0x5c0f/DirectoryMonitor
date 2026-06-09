// ── Events (Single Node Real-time) ──────────────────────
// This page shows real-time events from the current node only.
// For aggregated cluster events, use the "集群事件" tab.

// ── Target Type Filter ─────────────────────────────
let currentTargetType = ''; // '' = all, 'file', 'dir'

function setTargetType(type) {
  currentTargetType = type;

  // Update button states
  document.querySelectorAll('.target-type-option').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.value === type);
  });

  // Update label
  const label = type === 'file' ? 'FILE' : type === 'dir' ? 'DIR' : '所有目标';
  document.getElementById('targetTypeLabel').textContent = label;

  // Save to localStorage
  if (type) {
    localStorage.setItem('dm_target_type', type);
  } else {
    localStorage.removeItem('dm_target_type');
  }

  // Close dropdown
  document.getElementById('targetTypeDropdown').classList.remove('open');

  loadHistory(1);
}

// Toggle dropdown
document.getElementById('targetTypeBtn').addEventListener('click', (e) => {
  e.stopPropagation();
  const dropdown = document.getElementById('targetTypeDropdown');
  const isOpen = dropdown.classList.toggle('open');
  document.getElementById('targetTypeBtn').setAttribute('aria-expanded', isOpen);
});

// Close dropdown when clicking outside
document.addEventListener('click', (e) => {
  const dropdown = document.getElementById('targetTypeDropdown');
  if (!dropdown.contains(e.target)) {
    dropdown.classList.remove('open');
    document.getElementById('targetTypeBtn').setAttribute('aria-expanded', 'false');
  }
});

// Restore from localStorage
(function() {
  const saved = localStorage.getItem('dm_target_type');
  if (saved) {
    setTargetType(saved);
  }
})();

// ── Event Type Filter ─────────────────────────────────
let selectedTypes = new Set(); // empty = all types

function initTypeCheckboxes() {
  // 从 localStorage 恢复选中状态
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem('dm_event_types')); } catch {}
  const savedSet = saved ? new Set(saved) : null;

  typeCheckboxes.innerHTML = EVENT_TYPES.map(type => {
    const checked = !savedSet || savedSet.has(type) ? 'checked' : '';
    return `<div class="checkbox-item">
      <input type="checkbox" id="type-${type}" value="${type}" ${checked}>
      <label for="type-${type}">${type}</label>
    </div>`;
  }).join('');

  // 初始化 selectedTypes
  if (savedSet) {
    selectedTypes = savedSet;
  }

  // Update label
  updateSelectedTypes();

  // Add change listeners
  typeCheckboxes.querySelectorAll('input[type="checkbox"]').forEach(cb => {
    cb.addEventListener('change', () => {
      updateSelectedTypes();
      loadHistory(1);
    });
  });
}

function updateSelectedTypes() {
  const checked = typeCheckboxes.querySelectorAll('input:checked');
  selectedTypes.clear();
  checked.forEach(cb => selectedTypes.add(cb.value));
  // 持久化到 localStorage
  if (selectedTypes.size === EVENT_TYPES.length || selectedTypes.size === 0) {
    localStorage.removeItem('dm_event_types');
  } else {
    localStorage.setItem('dm_event_types', JSON.stringify([...selectedTypes]));
  }

  // Update label
  const count = selectedTypes.size;
  if (count === 0) {
    typeFilterLabel.textContent = '未选择';
  } else if (count === EVENT_TYPES.length) {
    typeFilterLabel.textContent = '所有类型';
  } else if (count <= 2) {
    typeFilterLabel.textContent = [...selectedTypes].join(', ');
  } else {
    typeFilterLabel.textContent = count + ' 个类型';
  }
}

function selectAllTypes() {
  typeCheckboxes.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = true);
  updateSelectedTypes();
  loadHistory(1);
}

function clearAllTypes() {
  typeCheckboxes.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = false);
  updateSelectedTypes();
  loadHistory(1);
}

// Toggle dropdown
typeFilterBtn.addEventListener('click', (e) => {
  e.stopPropagation();
  const isOpen = typeFilterDropdown.classList.toggle('open');
  typeFilterBtn.setAttribute('aria-expanded', isOpen);
});

// Close dropdown when clicking outside
document.addEventListener('click', (e) => {
  if (!typeFilterDropdown.contains(e.target)) {
    typeFilterDropdown.classList.remove('open');
    typeFilterBtn.setAttribute('aria-expanded', 'false');
  }
});

// Initialize
initTypeCheckboxes();

// ── Events ───────────────────────────────────────────
function escHtml(s) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function formatTime(ts) {
  try { return new Date(ts).toLocaleTimeString('zh-CN', { hour12: false }); }
  catch { return ts; }
}

function renderEvents() {
  // 合并实时事件和分页事件（搜索和类型过滤已由服务端处理）
  const allEvents = [...liveEvents, ...events];

  countEl.textContent = allEvents.length + ' 条事件';
  if (allEvents.length === 0) {
    list.innerHTML = '<div class="empty-msg"><div class="empty-icon"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="color: var(--text-muted)"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></div>等待文件系统事件...</div>';
    return;
  }
  let html = '';
  for (const e of allEvents) {
    const typeLabel = e.is_dir === true ? 'DIR' : e.is_dir === false ? 'FILE' : '-';
    const typeClass = e.is_dir === true ? 'type-dir' : e.is_dir === false ? 'type-file' : 'type-unknown';
    html += '<div class="event-item">' +
      '<span class="event-time">' + formatTime(e.timestamp) + '</span>' +
      '<span class="event-type t-' + e.event_type + '">' + e.event_type + '</span>' +
      '<span class="event-target-type ' + typeClass + '">' + typeLabel + '</span>' +
      '<span class="event-path-wrap"><span class="event-path" data-tip="' + escHtml(e.path) + '">' + escHtml(e.path) + '</span></span>' +
      (e.target_path ? '<span class="event-target">→ ' + escHtml(e.target_path) + '</span>' : '') +
      '</div>';
  }
  list.innerHTML = html;
}

function addEvent(e) {
  // 实时事件添加到 liveEvents，保持最新在前
  liveEvents.unshift(e);
  if (liveEvents.length > PER_PAGE) liveEvents.pop();
  renderEvents();
}

// Path toast: tap to show full path at bottom
(function() {
  const toast = document.createElement('div');
  toast.className = 'path-toast';
  toast.innerHTML =
    '<span class="path-toast-text"></span>' +
    '<button class="path-toast-btn" id="pathCopyBtn" title="复制路径">' +
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>' +
    '</button>';
  document.body.appendChild(toast);

  const textEl = toast.querySelector('.path-toast-text');
  const copyBtn = document.getElementById('pathCopyBtn');

  list.addEventListener('click', (e) => {
    const path = e.target.closest('.event-path[data-tip]');
    if (!path) return;
    e.stopPropagation();
    const full = path.getAttribute('data-tip');
    textEl.textContent = full;
    toast.classList.add('visible');
    copyBtn.classList.remove('copied');
  });

  copyBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    const text = textEl.textContent;
    if (navigator.clipboard) {
      navigator.clipboard.writeText(text).then(() => {
        copyBtn.classList.add('copied');
        setTimeout(() => copyBtn.classList.remove('copied'), 1500);
      });
    }
  });

  // 点击其他地方关闭
  document.addEventListener('click', (e) => {
    if (!e.target.closest('.event-path[data-tip]') && !e.target.closest('.path-toast')) {
      toast.classList.remove('visible');
    }
  });
})();

function connect() {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  let url = proto + '//' + location.host + '/ws';
  if (token) url += '?token=' + encodeURIComponent(token);
  ws = new WebSocket(url);
  ws.onopen = () => {
    dot.classList.add('connected');
    statusText.textContent = '在线';
    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
  };
  ws.onmessage = (msg) => {
    try { addEvent(JSON.parse(msg.data)); } catch {}
  };
  ws.onclose = () => {
    dot.classList.remove('connected');
    statusText.textContent = '离线';
    reconnectTimer = setTimeout(connect, 2000);
  };
  ws.onerror = () => ws.close();
}

async function loadHistory(page) {
  page = page || currentPage || 1;
  const searchVal = searchInput.value.trim();
  let url = '/api/events?page=' + page + '&per_page=' + PER_PAGE;
  if (searchVal) url += '&search=' + encodeURIComponent(searchVal);
  // 传递事件类型过滤（非全选时）
  if (selectedTypes.size > 0 && selectedTypes.size < EVENT_TYPES.length) {
    url += '&types=' + encodeURIComponent([...selectedTypes].join(','));
  }
  // 传递时间范围
  if (currentTimeAfter) {
    url += '&after=' + encodeURIComponent(currentTimeAfter);
  }
  if (currentTimeBefore) {
    url += '&before=' + encodeURIComponent(currentTimeBefore);
  }
  // 传递目标类型过滤
  if (currentTargetType) {
    url += '&target_type=' + encodeURIComponent(currentTargetType);
  }
  try {
    const resp = await fetch(url, { headers: authHeaders() });
    const data = await resp.json();
    if (data.events) {
      events = data.events;
      liveEvents = [];  // 加载新页时清除实时事件
      currentPage = data.page || 1;
      totalPages = data.total_pages || 1;
      localStorage.setItem('dm_events_page', currentPage);
      updatePagination(data.total || 0);
      renderEvents();
    }
  } catch {}
}

function goToPage(page) {
  if (page < 1 || page > totalPages) return;
  loadHistory(page);
}

function jumpToPage() {
  const input = document.getElementById('page-jump-input');
  const page = parseInt(input.value, 10);
  if (isNaN(page) || page < 1) { input.value = currentPage; return; }
  const target = Math.min(page, totalPages);
  if (target !== currentPage) goToPage(target);
  input.value = target;
}

function updatePagination(total) {
  const pag = document.getElementById('pagination');
  if (total <= PER_PAGE && liveEvents.length === 0) {
    pag.style.display = 'none';
    return;
  }
  pag.style.display = 'flex';
  document.getElementById('page-jump-input').value = currentPage;
  document.getElementById('page-jump-input').max = totalPages;
  document.getElementById('total-pages').textContent = totalPages;
  document.getElementById('total-count').textContent = total;
  document.getElementById('btn-first').disabled = currentPage <= 1;
  document.getElementById('btn-prev').disabled = currentPage <= 1;
  document.getElementById('btn-next').disabled = currentPage >= totalPages;
  document.getElementById('btn-last').disabled = currentPage >= totalPages;
}

searchInput.addEventListener('input', () => {
  clearTimeout(searchDebounce);
  searchDebounce = setTimeout(() => { loadHistory(1); }, 300);
});

// ── Time Range Filter ───────────────────────────────
let currentTimeAfter = null;
let currentTimeBefore = null;

function setTimeRange(range) {
  // Update button states
  document.querySelectorAll('.time-preset').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.range === range);
  });

  // Hide custom time panel
  document.getElementById('customTime').style.display = 'none';

  const now = new Date();
  switch (range) {
    case 'all':
      currentTimeAfter = null;
      currentTimeBefore = null;
      break;
    case '1h':
      currentTimeAfter = new Date(now.getTime() - 60 * 60 * 1000).toISOString();
      currentTimeBefore = null;
      break;
    case 'today':
      const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      currentTimeAfter = today.toISOString();
      currentTimeBefore = null;
      break;
    case '7d':
      currentTimeAfter = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000).toISOString();
      currentTimeBefore = null;
      break;
  }

  loadHistory(1);
}

function toggleCustomTime() {
  const panel = document.getElementById('customTime');
  const isVisible = panel.style.display !== 'none';
  panel.style.display = isVisible ? 'none' : 'flex';

  // Update button state
  document.querySelectorAll('.time-preset').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.range === 'custom');
  });

  if (!isVisible) {
    // Set default values
    const now = new Date();
    const yesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000);
    document.getElementById('timeAfter').value = formatDateTimeLocal(yesterday);
    document.getElementById('timeBefore').value = formatDateTimeLocal(now);
  }
}

function formatDateTimeLocal(date) {
  return date.getFullYear() + '-' +
    String(date.getMonth() + 1).padStart(2, '0') + '-' +
    String(date.getDate()).padStart(2, '0') + 'T' +
    String(date.getHours()).padStart(2, '0') + ':' +
    String(date.getMinutes()).padStart(2, '0');
}

function applyCustomTime() {
  const afterInput = document.getElementById('timeAfter').value;
  const beforeInput = document.getElementById('timeBefore').value;

  if (afterInput) {
    currentTimeAfter = new Date(afterInput).toISOString();
  }
  if (beforeInput) {
    currentTimeBefore = new Date(beforeInput).toISOString();
  }

  loadHistory(1);
}
