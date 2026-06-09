// ── Cluster Events ───────────────────────────────────────
// Aggregated event query across cluster nodes

let currentClusterPage = 1;
let totalClusterPages = 1;
let clusterSearchDebounce = null;
let clusterEventsInitialized = false;

// Cluster event filters
let selectedClusterTypes = new Set();
let selectedClusterNodes = new Set();
let availableClusterNodes = [];
let currentClusterTimeAfter = null;
let currentClusterTimeBefore = null;

// Debounced query trigger
let clusterQueryDebounce = null;
function triggerClusterQuery() {
  clearTimeout(clusterQueryDebounce);
  clusterQueryDebounce = setTimeout(() => { loadClusterEvents(1); }, 200);
}

// ── Cluster Type Filter ─────────────────────────────────
function initClusterTypeCheckboxes() {
  const container = document.getElementById('clusterTypeCheckboxes');
  if (!container) return;

  // 从 localStorage 恢复选中状态
  let saved = null;
  try { saved = JSON.parse(localStorage.getItem('dm_cluster_event_types')); } catch {}
  const savedSet = saved ? new Set(saved) : null;

  container.innerHTML = EVENT_TYPES.map(type => {
    const checked = !savedSet || savedSet.has(type) ? 'checked' : '';
    return `<div class="checkbox-item">
      <input type="checkbox" id="cluster-type-${type}" value="${type}" ${checked}>
      <label for="cluster-type-${type}">${type}</label>
    </div>`;
  }).join('');

  // 初始化 selectedClusterTypes
  if (savedSet) {
    selectedClusterTypes = savedSet;
  }

  // Update label
  updateSelectedClusterTypes();

  // Add change listeners with auto-query
  container.querySelectorAll('input[type="checkbox"]').forEach(cb => {
    cb.addEventListener('change', () => {
      updateSelectedClusterTypes();
      triggerClusterQuery();
    });
  });
}

function updateSelectedClusterTypes() {
  const container = document.getElementById('clusterTypeCheckboxes');
  const label = document.getElementById('clusterTypeFilterLabel');
  if (!container || !label) return;

  const checked = container.querySelectorAll('input:checked');
  selectedClusterTypes.clear();
  checked.forEach(cb => selectedClusterTypes.add(cb.value));

  // 持久化到 localStorage
  if (selectedClusterTypes.size === EVENT_TYPES.length || selectedClusterTypes.size === 0) {
    localStorage.removeItem('dm_cluster_event_types');
  } else {
    localStorage.setItem('dm_cluster_event_types', JSON.stringify([...selectedClusterTypes]));
  }

  // Update label
  const count = selectedClusterTypes.size;
  if (count === 0) {
    label.textContent = '未选择';
  } else if (count === EVENT_TYPES.length) {
    label.textContent = '所有类型';
  } else if (count <= 2) {
    label.textContent = [...selectedClusterTypes].join(', ');
  } else {
    label.textContent = count + ' 个类型';
  }
}

function selectAllClusterTypes() {
  const container = document.getElementById('clusterTypeCheckboxes');
  if (container) {
    container.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = true);
  }
  updateSelectedClusterTypes();
  triggerClusterQuery();
}

function clearAllClusterTypes() {
  const container = document.getElementById('clusterTypeCheckboxes');
  if (container) {
    container.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = false);
  }
  updateSelectedClusterTypes();
  triggerClusterQuery();
}

// ── Cluster Node Filter ─────────────────────────────────
async function initClusterNodeCheckboxes() {
  try {
    const token = localStorage.getItem('dm_token');
    const headers = token ? { 'Authorization': 'Bearer ' + token } : {};
    const resp = await fetch('/api/cluster/nodes', { headers });
    if (!resp.ok) return;

    availableClusterNodes = await resp.json();
    const container = document.getElementById('clusterNodeCheckboxes');
    if (!container) return;

    container.innerHTML = availableClusterNodes.map(node => {
      const checked = selectedClusterNodes.size === 0 || selectedClusterNodes.has(node.id) ? 'checked' : '';
      return `<div class="checkbox-item">
        <input type="checkbox" id="cluster-node-${node.id}" value="${node.id}" ${checked}>
        <label for="cluster-node-${node.id}">${escHtml(node.name || node.id)}</label>
      </div>`;
    }).join('');

    updateSelectedClusterNodes();

    container.querySelectorAll('input[type="checkbox"]').forEach(cb => {
      cb.addEventListener('change', () => {
        updateSelectedClusterNodes();
        triggerClusterQuery();
      });
    });
  } catch (e) {
    console.error('Failed to load cluster nodes:', e);
  }
}

function updateSelectedClusterNodes() {
  const container = document.getElementById('clusterNodeCheckboxes');
  const label = document.getElementById('clusterNodeFilterLabel');
  if (!container || !label) return;

  const checked = container.querySelectorAll('input:checked');
  selectedClusterNodes.clear();
  checked.forEach(cb => selectedClusterNodes.add(cb.value));

  const count = selectedClusterNodes.size;
  const totalCount = availableClusterNodes.length;
  if (count === 0 || count === totalCount) {
    label.textContent = '所有节点';
  } else if (count <= 2) {
    label.textContent = [...selectedClusterNodes].map(id => {
      const node = availableClusterNodes.find(n => n.id === id);
      return node ? (node.name || node.id) : id;
    }).join(', ');
  } else {
    label.textContent = count + ' 个节点';
  }
}

function selectAllClusterNodes() {
  const container = document.getElementById('clusterNodeCheckboxes');
  if (container) {
    container.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = true);
  }
  updateSelectedClusterNodes();
  triggerClusterQuery();
}

function clearAllClusterNodes() {
  const container = document.getElementById('clusterNodeCheckboxes');
  if (container) {
    container.querySelectorAll('input[type="checkbox"]').forEach(cb => cb.checked = false);
  }
  updateSelectedClusterNodes();
  triggerClusterQuery();
}

// ── Cluster Time Filter ─────────────────────────────────
function setClusterTimeRange(range) {
  document.querySelectorAll('#tab-cluster-events .time-preset').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.range === range);
  });

  document.getElementById('clusterCustomTime').style.display = 'none';

  const now = new Date();
  switch (range) {
    case 'all':
      currentClusterTimeAfter = null;
      currentClusterTimeBefore = null;
      break;
    case '1h':
      currentClusterTimeAfter = new Date(now.getTime() - 60 * 60 * 1000).toISOString();
      currentClusterTimeBefore = null;
      break;
    case 'today':
      const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
      currentClusterTimeAfter = today.toISOString();
      currentClusterTimeBefore = null;
      break;
    case '7d':
      currentClusterTimeAfter = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000).toISOString();
      currentClusterTimeBefore = null;
      break;
  }

  // Auto-query on time range change
  triggerClusterQuery();
}

function toggleClusterCustomTime() {
  const panel = document.getElementById('clusterCustomTime');
  const isVisible = panel.style.display !== 'none';
  panel.style.display = isVisible ? 'none' : 'flex';

  document.querySelectorAll('#tab-cluster-events .time-preset').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.range === 'custom');
  });

  if (!isVisible) {
    const now = new Date();
    const yesterday = new Date(now.getTime() - 24 * 60 * 60 * 1000);
    document.getElementById('clusterTimeAfter').value = formatDateTimeLocal(yesterday);
    document.getElementById('clusterTimeBefore').value = formatDateTimeLocal(now);
  }
}

function applyClusterCustomTime() {
  const afterInput = document.getElementById('clusterTimeAfter').value;
  const beforeInput = document.getElementById('clusterTimeBefore').value;

  if (afterInput) {
    currentClusterTimeAfter = new Date(afterInput).toISOString();
  }
  if (beforeInput) {
    currentClusterTimeBefore = new Date(beforeInput).toISOString();
  }

  loadClusterEvents(1);
}

// ── Cluster Events Query ────────────────────────────────
async function loadClusterEvents(page) {
  page = page || currentClusterPage || 1;
  const searchInput = document.getElementById('cluster-search');
  const searchVal = searchInput ? searchInput.value.trim() : '';

  let url = '/api/events?page=' + page + '&per_page=' + PER_PAGE;
  if (searchVal) url += '&search=' + encodeURIComponent(searchVal);

  // 传递事件类型过滤（非全选时）
  if (selectedClusterTypes.size > 0 && selectedClusterTypes.size < EVENT_TYPES.length) {
    url += '&types=' + encodeURIComponent([...selectedClusterTypes].join(','));
  }

  // 传递时间范围
  if (currentClusterTimeAfter) {
    url += '&after=' + encodeURIComponent(currentClusterTimeAfter);
  }
  if (currentClusterTimeBefore) {
    url += '&before=' + encodeURIComponent(currentClusterTimeBefore);
  }

  // 传递节点过滤
  if (selectedClusterNodes.size > 0 && selectedClusterNodes.size < availableClusterNodes.length) {
    url += '&node_id=' + encodeURIComponent([...selectedClusterNodes].join(','));
  }

  try {
    const token = localStorage.getItem('dm_token');
    const headers = token ? { 'Authorization': 'Bearer ' + token } : {};
    const resp = await fetch(url, { headers });
    const data = await resp.json();

    if (data.events) {
      currentClusterPage = data.page || 1;
      totalClusterPages = data.total_pages || 1;
      renderClusterEvents(data.events);
      updateClusterPagination(data.total || 0);
    }
  } catch (e) {
    console.error('Failed to load cluster events:', e);
  }
}

function renderClusterEvents(events) {
  const list = document.getElementById('cluster-list');
  const countEl = document.getElementById('cluster-count');
  if (!list || !countEl) return;

  countEl.textContent = events.length + ' 条事件';

  if (events.length === 0) {
    list.innerHTML = '<div class="empty-msg"><div class="empty-icon"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="color: var(--text-muted)"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></div>暂无匹配事件</div>';
    return;
  }

  let html = '';
  for (const e of events) {
    const typeLabel = e.is_dir === true ? 'DIR' : e.is_dir === false ? 'FILE' : '-';
    const typeClass = e.is_dir === true ? 'type-dir' : e.is_dir === false ? 'type-file' : 'type-unknown';
    html += '<div class="event-item">' +
      '<span class="event-time">' + formatTime(e.timestamp) + '</span>' +
      '<span class="event-type t-' + e.event_type + '">' + e.event_type + '</span>' +
      '<span class="event-target-type ' + typeClass + '">' + typeLabel + '</span>' +
      '<span class="event-path-wrap"><span class="event-path" data-tip="' + escHtml(e.path) + '">' + escHtml(e.path) + '</span></span>' +
      '<span class="event-node" title="' + escHtml(e.node_id || '') + '">' + escHtml(e.node_name || e.node_id || 'local') + '</span>' +
      (e.target_path ? '<span class="event-target">→ ' + escHtml(e.target_path) + '</span>' : '') +
      '</div>';
  }
  list.innerHTML = html;
}

function updateClusterPagination(total) {
  const pag = document.getElementById('cluster-pagination');
  if (!pag) return;

  if (total <= PER_PAGE) {
    pag.style.display = 'none';
    return;
  }

  pag.style.display = 'flex';
  document.getElementById('cluster-page-jump-input').value = currentClusterPage;
  document.getElementById('cluster-page-jump-input').max = totalClusterPages;
  document.getElementById('cluster-total-pages').textContent = totalClusterPages;
  document.getElementById('cluster-total-count').textContent = total;
  document.getElementById('cluster-btn-first').disabled = currentClusterPage <= 1;
  document.getElementById('cluster-btn-prev').disabled = currentClusterPage <= 1;
  document.getElementById('cluster-btn-next').disabled = currentClusterPage >= totalClusterPages;
  document.getElementById('cluster-btn-last').disabled = currentClusterPage >= totalClusterPages;
}

function goToClusterPage(page) {
  if (page < 1 || page > totalClusterPages) return;
  loadClusterEvents(page);
}

function jumpToClusterPage() {
  const input = document.getElementById('cluster-page-jump-input');
  const page = parseInt(input.value, 10);
  if (isNaN(page) || page < 1) { input.value = currentClusterPage; return; }
  const target = Math.min(page, totalClusterPages);
  if (target !== currentClusterPage) goToClusterPage(target);
  input.value = target;
}

// ── Initialize ──────────────────────────────────────────
function initClusterEventsTab() {
  // Only initialize once to avoid duplicate event listeners
  if (clusterEventsInitialized) return;
  clusterEventsInitialized = true;

  initClusterTypeCheckboxes();
  initClusterNodeCheckboxes();

  // Search input with debounce
  const searchInput = document.getElementById('cluster-search');
  if (searchInput) {
    searchInput.addEventListener('input', () => {
      clearTimeout(clusterSearchDebounce);
      clusterSearchDebounce = setTimeout(() => { loadClusterEvents(1); }, 300);
    });
  }

  // Toggle dropdowns
  const typeBtn = document.getElementById('clusterTypeFilterBtn');
  const typeDropdown = document.getElementById('clusterTypeFilterDropdown');
  if (typeBtn && typeDropdown) {
    typeBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = typeDropdown.classList.toggle('open');
      typeBtn.setAttribute('aria-expanded', isOpen);
    });
  }

  const nodeBtn = document.getElementById('clusterNodeFilterBtn');
  const nodeDropdown = document.getElementById('clusterNodeFilterDropdown');
  if (nodeBtn && nodeDropdown) {
    nodeBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = nodeDropdown.classList.toggle('open');
      nodeBtn.setAttribute('aria-expanded', isOpen);
    });
  }

  // Close dropdowns when clicking outside
  document.addEventListener('click', (e) => {
    if (typeDropdown && !typeDropdown.contains(e.target)) {
      typeDropdown.classList.remove('open');
      if (typeBtn) typeBtn.setAttribute('aria-expanded', 'false');
    }
    if (nodeDropdown && !nodeDropdown.contains(e.target)) {
      nodeDropdown.classList.remove('open');
      if (nodeBtn) nodeBtn.setAttribute('aria-expanded', 'false');
    }
  });

  // Initial load
  loadClusterEvents(1);
}

// Auto-initialize: handles both page restore and manual tab click
// This runs after auth.js because cluster-events.js loads later in the bundle
(function() {
  const clusterTabBtn = document.querySelector('.nav-tabs button[data-tab="cluster-events"]');
  if (!clusterTabBtn) return;

  // Add click listener for manual tab switch
  clusterTabBtn.addEventListener('click', () => {
    initClusterEventsTab();
  });

  // If this tab is already active (restored from localStorage), init now
  if (clusterTabBtn.classList.contains('active')) {
    initClusterEventsTab();
  }
})();
