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
  typeFilterDropdown.classList.toggle('open');
});

// Close dropdown when clicking outside
document.addEventListener('click', (e) => {
  if (!typeFilterDropdown.contains(e.target)) {
    typeFilterDropdown.classList.remove('open');
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
    list.innerHTML = '<div class="empty-msg"><div class="empty-icon"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="color: var(--text-muted)"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></div>暂无匹配事件</div>';
    return;
  }
  let html = '';
  for (const e of allEvents) {
    html += '<div class="event-item">' +
      '<span class="event-time">' + formatTime(e.timestamp) + '</span>' +
      '<span class="event-type t-' + e.event_type + '">' + e.event_type + '</span>' +
      '<span class="event-path" title="' + escHtml(e.path) + '">' + escHtml(e.path) + '</span>' +
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
