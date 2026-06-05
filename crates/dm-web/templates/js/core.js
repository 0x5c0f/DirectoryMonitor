// ── State ────────────────────────────────────────────
const PER_PAGE = 50;
let events = [];
let liveEvents = [];  // 实时 WebSocket 事件（未分页）
let ws = null;
let currentPage = parseInt(localStorage.getItem('dm_events_page') || '1', 10);
let totalPages = 1;
let searchDebounce = null;
let reconnectTimer = null;
let token = localStorage.getItem('dm_token');
let watches = [];
let originalWatches = [];  // 服务器端原始状态，用于重置
let dirtyFlags = [];        // 每个卡片的修改标记
let newFlags = [];          // 新增但未保存到后端的标记
let watchersPendingReload = false;  // 是否有未重载的配置变更

// 全局设置状态
let globalSettings = {};
let originalGlobalSettings = {};
let globalDirty = false;

// ── Theme ────────────────────────────────────────────
const THEME_ICONS = {
  light: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>',
  dark: '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>',
};

let currentTheme = localStorage.getItem('dm_theme') || 'dark';

function applyTheme(theme) {
  currentTheme = theme;
  document.documentElement.setAttribute('data-theme', theme);
  document.body.className = 'theme-' + theme;
  localStorage.setItem('dm_theme', theme);

  // Update header theme icon
  const themeIcon = document.getElementById('themeIcon');
  if (themeIcon) {
    themeIcon.outerHTML = THEME_ICONS[theme === 'dark' ? 'light' : 'dark'];
    const btn = document.getElementById('themeBtn');
    if (btn) btn.querySelector('svg').id = 'themeIcon';
  }

  // Update login page theme icon
  const loginThemeIcon = document.getElementById('loginThemeIcon');
  if (loginThemeIcon) {
    loginThemeIcon.outerHTML = THEME_ICONS[theme === 'dark' ? 'light' : 'dark'];
    const loginBtn = document.getElementById('loginThemeBtn');
    if (loginBtn) loginBtn.querySelector('svg').id = 'loginThemeIcon';
  }
}

function toggleTheme() {
  applyTheme(currentTheme === 'dark' ? 'light' : 'dark');
}

// Apply saved theme on load
applyTheme(currentTheme);

const EVENT_TYPES = ['CREATE','MODIFY','ATTRIB','CLOSE_WRITE','CLOSE_NOWRITE','OPEN','MOVED_TO','MOVED_FROM','DELETE','RENAME','ACCESS'];

// 事件类型名称映射（处理过去式/现在式的差异）
const EVENT_TYPE_ALIASES = {
  'created': 'CREATE', 'modified': 'MODIFY', 'attrib': 'ATTRIB',
  'close_write': 'CLOSE_WRITE', 'close_nowrite': 'CLOSE_NOWRITE', 'open': 'OPEN',
  'moved_to': 'MOVED_TO', 'moved_from': 'MOVED_FROM', 'deleted': 'DELETE',
  'renamed': 'RENAME', 'access': 'ACCESS',
  // 同时支持大写原形
  'create': 'CREATE', 'modify': 'MODIFY', 'delete': 'DELETE', 'rename': 'RENAME',
};

// 规范化事件类型名称（转为标准大写形式）
function normalizeEventType(name) {
  const upper = name.toUpperCase();
  return EVENT_TYPE_ALIASES[upper] || EVENT_TYPE_ALIASES[name.toLowerCase()] || upper;
}

// 检查事件类型是否匹配
function eventTypeMatches(configType, displayType) {
  return normalizeEventType(configType) === displayType;
}

// ── SVG Icons ────────────────────────────────────────
const ICONS = {
  check: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>',
  close: '&times;',
};

// ── DOM refs ─────────────────────────────────────────
const $ = id => document.getElementById(id);
const loginPage = $('loginPage');
const mainPage = $('mainPage');
const loginPassword = $('loginPassword');
const loginBtn = $('loginBtn');
const loginError = $('loginError');
const dot = $('dot');
const statusText = $('statusText');
const stats = $('stats');
const list = $('list');
const searchInput = $('search');
const typeFilterDropdown = $('typeFilterDropdown');
const typeFilterBtn = $('typeFilterBtn');
const typeFilterLabel = $('typeFilterLabel');
const typeCheckboxes = $('typeCheckboxes');
const countEl = $('count');
const settingsPanel = $('settingsPanel');
const logoutBtn = $('logoutBtn');
const themeBtn = $('themeBtn');
