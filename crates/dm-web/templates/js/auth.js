// ── Auth ─────────────────────────────────────────────
let authRequired = false;  // Track if authentication is configured

function authHeaders() {
  return token ? { 'Authorization': 'Bearer ' + token } : {};
}

async function checkAuth() {
  try {
    // First check if auth is required (no auth needed for this)
    const statusResp = await fetch('/api/auth/status');
    if (statusResp.ok) {
      const statusData = await statusResp.json();
      authRequired = statusData.auth_required || false;

      if (!authRequired) {
        // No password configured — go directly to main page
        token = null;
        localStorage.removeItem('dm_token');
        showMain();
        return;
      }
    }

    // Auth is required — check if we have a valid token
    if (!token) {
      showLogin();
      return;
    }

    const resp = await fetch('/api/auth/verify', { headers: authHeaders() });
    if (resp.ok) {
      const data = await resp.json();
      if (data.ok) {
        showMain();
        return;
      }
    }
    // Token invalid or expired — clear it
    token = null;
    localStorage.removeItem('dm_token');
    showLogin();
  } catch {
    showLogin();
  }
}

function hideLoading() {
  const el = document.getElementById('loadingScreen');
  if (!el) return;
  el.classList.add('fade-out');
  setTimeout(() => el.remove(), 250);
}

function showLogin() {
  hideLoading();
  loginPage.style.display = 'flex';
  mainPage.style.display = 'none';
  loginPassword.focus();
}

function showMain() {
  hideLoading();
  loginPage.style.display = 'none';
  mainPage.style.display = 'flex';
  loadHistory();
  connect();
  loadConfig();

  // Show/hide logout button based on auth requirement
  logoutBtn.style.display = authRequired ? 'flex' : 'none';

  // Initialize dashboard if it's the active tab
  const activeTab = localStorage.getItem('dm_active_tab') || 'dashboard';
  if (activeTab === 'dashboard') {
    initDashboard();
  }
}

loginBtn.addEventListener('click', doLogin);
loginPassword.addEventListener('keydown', e => { if (e.key === 'Enter') doLogin(); });

async function doLogin() {
  const pw = loginPassword.value;
  loginError.textContent = '';
  try {
    const resp = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password: pw })
    });
    if (resp.ok) {
      const data = await resp.json();
      if (data.token) {
        token = data.token;
        localStorage.setItem('dm_token', token);
      }
      showMain();
    } else {
      loginError.textContent = resp.status === 403 ? '密码错误' : '认证失败';
      loginPassword.select();
    }
  } catch {
    loginError.textContent = '连接服务器失败';
  }
}

logoutBtn.addEventListener('click', () => {
  token = null;
  localStorage.removeItem('dm_token');
  if (ws) ws.close();
  showLogin();
});

themeBtn.addEventListener('click', toggleTheme);

// Login page theme toggle
const loginThemeBtn = document.getElementById('loginThemeBtn');
if (loginThemeBtn) {
  loginThemeBtn.addEventListener('click', toggleTheme);
}

// ── Tabs ─────────────────────────────────────────────
function activateTab(tabName) {
  document.querySelectorAll('.nav-tabs button').forEach(b => {
    b.classList.remove('active');
    b.setAttribute('aria-selected', 'false');
  });
  document.querySelectorAll('.tab-content').forEach(t => t.classList.remove('active'));
  const targetBtn = document.querySelector('.nav-tabs button[data-tab="' + tabName + '"]');
  if (targetBtn) {
    targetBtn.classList.add('active');
    targetBtn.setAttribute('aria-selected', 'true');
    $('tab-' + tabName).classList.add('active');
  }
}

document.querySelectorAll('.nav-tabs button').forEach(btn => {
  btn.addEventListener('click', () => {
    const tabName = btn.dataset.tab;
    activateTab(tabName);
    localStorage.setItem('dm_active_tab', tabName);

    // Start/stop dashboard based on tab
    if (tabName === 'dashboard') {
      initDashboard();
    } else {
      stopDashboard();
    }
  });
});

// Restore active tab from localStorage
const savedTab = localStorage.getItem('dm_active_tab');
if (savedTab) activateTab(savedTab);
