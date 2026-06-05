// ── Auth ─────────────────────────────────────────────
function authHeaders() {
  return token ? { 'Authorization': 'Bearer ' + token } : {};
}

async function checkAuth() {
  try {
    // First check if auth is required (no auth needed for this)
    const statusResp = await fetch('/api/auth/status');
    if (statusResp.ok) {
      const statusData = await statusResp.json();
      if (!statusData.auth_required) {
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
  document.querySelectorAll('.nav-tabs button').forEach(b => b.classList.remove('active'));
  document.querySelectorAll('.tab-content').forEach(t => t.classList.remove('active'));
  const targetBtn = document.querySelector('.nav-tabs button[data-tab="' + tabName + '"]');
  if (targetBtn) {
    targetBtn.classList.add('active');
    $('tab-' + tabName).classList.add('active');
  }
}

document.querySelectorAll('.nav-tabs button').forEach(btn => {
  btn.addEventListener('click', () => {
    activateTab(btn.dataset.tab);
    localStorage.setItem('dm_active_tab', btn.dataset.tab);
  });
});

// Restore active tab from localStorage
const savedTab = localStorage.getItem('dm_active_tab');
if (savedTab) activateTab(savedTab);
