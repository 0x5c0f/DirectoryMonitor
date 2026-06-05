// ── Settings ─────────────────────────────────────────
const LOG_LEVELS = ['trace', 'debug', 'info', 'warn', 'error'];

async function loadConfig() {
  try {
    const resp = await fetch('/api/config', { headers: authHeaders() });
    const data = await resp.json();
    watches = data.watches || [];
    // 深拷贝保存原始状态
    originalWatches = JSON.parse(JSON.stringify(watches));
    dirtyFlags = new Array(watches.length).fill(false);
    newFlags = new Array(watches.length).fill(false);
    // 从服务端获取是否有未重载的配置变更
    watchersPendingReload = data.pending_reload || false;

    // 保存全局设置（包含通知设置）
    globalSettings = {
      logging_level: data.logging_level || 'info',
      database_enabled: data.database_enabled !== false,
      database_path: data.database_path || 'directory-monitor.db',
      email_enabled: data.email_enabled || false,
      email_smtp_server: data.email_smtp_server || '',
      email_smtp_port: data.email_smtp_port || 587,
      email_username: data.email_username || '',
      email_password: data.email_password || '',
      email_batch_size: data.email_batch_size || 10,
      email_max_per_minute: data.email_max_per_minute || 10,
      syslog_enabled: data.syslog_enabled || false,
      syslog_server: data.syslog_server || 'localhost',
      syslog_port: data.syslog_port || 514,
      syslog_format: data.syslog_format || 'rfc5424',
    };
    originalGlobalSettings = JSON.parse(JSON.stringify(globalSettings));
    globalDirty = false;

    renderSettings();
  } catch {}
}

function updateReloadButton() {
  const btn = document.getElementById('reloadBtn');
  if (btn) {
    if (watchersPendingReload) {
      btn.classList.add('pending');
      btn.title = '有未生效的配置变更，点击重载';
    } else {
      btn.classList.remove('pending');
      btn.title = '重新加载监控配置';
    }
  }
}

function setPendingReload() {
  watchersPendingReload = true;
  updateReloadButton();
}

function clearPendingReload() {
  watchersPendingReload = false;
  updateReloadButton();
}

function renderSettings() {
  let html = '<div class="settings-toolbar">' +
    '<div class="settings-toolbar-left">' +
      '<button class="btn-reload' + (watchersPendingReload ? ' pending' : '') + '" id="reloadBtn" onclick="reloadWatchers()" title="' + (watchersPendingReload ? '有未生效的配置变更，点击重载' : '重新加载监控配置') + '">' +
        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>' +
        '重载监控' +
      '</button>' +
      '<span class="reload-hint">修改监控目标后需要重载才能生效</span>' +
    '</div>' +
    '<button class="btn-add-watch" onclick="showAddWatchDialog()">' +
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>' +
      '添加监控目标' +
    '</button>' +
  '</div>';

  if (watches.length === 0) {
    html += '<div class="empty-msg"><div class="empty-icon">?</div>未配置监控目录，点击上方按钮添加</div>';
  }

  watches.forEach((w, idx) => {
    const isDirty = dirtyFlags[idx] || false;
    const isNew = newFlags[idx] || false;
    html += '<div class="watch-card' + (isNew ? ' new' : '') + '" id="watch-card-' + idx + '">' +
      '<div class="watch-card-header">' +
        '<span class="watch-path">' + escHtml(w.path) + '</span>' +
        '<span class="watch-badge">Watch #' + (idx + 1) + '</span>' +
        '<span class="dirty-indicator' + (isDirty ? ' show' : '') + '" id="dirty-' + idx + '" title="有未保存的更改"></span>' +
        '<button class="btn-delete-watch" onclick="deleteWatch(' + idx + ')" title="删除此监控目标">' +
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>' +
        '</button>' +
      '</div>' +

      // Recursive toggle
      '<div class="field-group">' +
        '<div class="field-label">递归监控</div>' +
        '<label class="toggle"><input type="checkbox" ' + (w.recursive ? 'checked' : '') +
          ' onchange="updateWatch(' + idx + ', \'recursive\', this.checked)">' +
          '<span class="toggle-slider"></span></label>' +
      '</div>' +

      // Event types
      '<div class="field-group">' +
        '<div class="field-label">事件类型 <span>(留空 = 全部)</span></div>' +
        '<div class="checkbox-group">';
    EVENT_TYPES.forEach(t => {
      // 使用规范化函数比较（处理大小写和时态差异）
      const checked = w.event_types.length === 0 || w.event_types.some(e => eventTypeMatches(e, t));
      html += '<label class="checkbox-label ' + (checked ? 'checked' : '') + '">' +
        '<input type="checkbox" ' + (checked ? 'checked' : '') +
        ' data-type="' + t + '" onchange="toggleEventType(' + idx + ', this)">' + t + '</label>';
    });
    html += '</div></div>' +

      // Include patterns
      '<div class="field-group">' +
        '<div class="field-label">包含模式 <span>(留空 = 全部文件)</span></div>' +
        '<div class="tag-list" id="include-' + idx + '">';
    w.include.forEach(p => {
      html += '<span class="tag">' + escHtml(p) + '<span class="remove" onclick="removeTag(' + idx + ',\'include\',\'' + escHtml(p) + '\')">' + ICONS.close + '</span></span>';
    });
    html += '<input class="tag-input" placeholder="输入后按 Enter" onkeydown="addTagKey(event,' + idx + ',\'include\')"></div></div>' +

      // Exclude patterns
      '<div class="field-group">' +
        '<div class="field-label">排除模式</div>' +
        '<div class="tag-list" id="exclude-' + idx + '">';
    w.exclude.forEach(p => {
      html += '<span class="tag">' + escHtml(p) + '<span class="remove" onclick="removeTag(' + idx + ',\'exclude\',\'' + escHtml(p) + '\')">' + ICONS.close + '</span></span>';
    });
    html += '<input class="tag-input" placeholder="输入后按 Enter" onkeydown="addTagKey(event,' + idx + ',\'exclude\')"></div></div>' +

      // Save/Reset buttons
      '<div class="card-actions">' +
        '<button class="btn-save" id="btn-save-' + idx + '" ' + (isDirty ? '' : 'disabled') + ' onclick="saveCard(' + idx + ')">' +
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>' +
          '保存' +
        '</button>' +
        '<button class="btn-reset" id="btn-reset-' + idx + '" ' + (isDirty ? '' : 'disabled') + ' onclick="resetCard(' + idx + ')">' +
          '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>' +
          '重置' +
        '</button>' +
        '<div class="save-status" id="save-status-' + idx + '">' + ICONS.check + ' 已保存到配置文件</div>' +
      '</div>' +
    '</div>';
  });

  // 全局设置卡片
  html += '<div class="watch-card" id="global-settings-card">' +
    '<div class="watch-card-header">' +
      '<span class="watch-path">全局设置</span>' +
      '<span class="watch-badge">GLOBAL</span>' +
      '<span class="dirty-indicator' + (globalDirty ? ' show' : '') + '" id="dirty-global" title="有未保存的更改"></span>' +
    '</div>' +

    // ── 系统设置 ──
    '<div class="settings-section">' +
      '<div class="section-title">系统设置</div>' +

      // 日志级别和数据库开关
      '<div class="field-row">' +
        '<div class="field-group">' +
          '<div class="field-label">日志级别</div>' +
          '<select class="select-input" id="global-logging-level" onchange="updateGlobalSetting(\'logging_level\', this.value)">';
  LOG_LEVELS.forEach(level => {
    html += '<option value="' + level + '"' + (globalSettings.logging_level === level ? ' selected' : '') + '>' + level + '</option>';
  });
  html += '</select></div>' +
        '<div class="field-group">' +
          '<div class="field-label">数据库存储</div>' +
          '<label class="toggle"><input type="checkbox" ' + (globalSettings.database_enabled ? 'checked' : '') +
            ' onchange="updateGlobalSetting(\'database_enabled\', this.checked)">' +
            '<span class="toggle-slider"></span></label>' +
        '</div>' +
      '</div>' +

      // 数据库路径
      '<div class="field-group" id="database-path-group" style="' + (globalSettings.database_enabled ? '' : 'display:none') + '">' +
        '<div class="field-label">数据库路径</div>' +
        '<input class="text-input" value="' + escHtml(globalSettings.database_path) + '" ' +
          'onchange="updateGlobalSetting(\'database_path\', this.value)" placeholder="directory-monitor.db">' +
      '</div>' +
    '</div>' +

    // ── 邮件通知 ──
    '<div class="settings-section">' +
      '<div class="section-title">邮件通知</div>' +

      // 邮件通知开关
      '<div class="field-group">' +
        '<label class="toggle"><input type="checkbox" ' + (globalSettings.email_enabled ? 'checked' : '') +
          ' onchange="updateGlobalSetting(\'email_enabled\', this.checked)">' +
          '<span class="toggle-slider"></span></label>' +
        '<span class="toggle-label">启用邮件通知</span>' +
      '</div>' +

      // 邮件设置详情（仅在启用时显示）
      '<div id="email-settings-details" style="' + (globalSettings.email_enabled ? '' : 'display:none') + '">' +
        // 第一行：服务器和端口
        '<div class="field-row">' +
          '<div class="field-group">' +
            '<div class="field-label">SMTP 服务器</div>' +
            '<input class="text-input" value="' + escHtml(globalSettings.email_smtp_server) + '" ' +
              'onchange="updateGlobalSetting(\'email_smtp_server\', this.value)" placeholder="smtp.gmail.com">' +
          '</div>' +
          '<div class="field-group">' +
            '<div class="field-label">SMTP 端口</div>' +
            '<input class="text-input" type="number" value="' + globalSettings.email_smtp_port + '" ' +
              'onchange="updateGlobalSetting(\'email_smtp_port\', parseInt(this.value))" placeholder="587">' +
          '</div>' +
        '</div>' +
        // 第二行：用户名和密码
        '<div class="field-row">' +
          '<div class="field-group">' +
            '<div class="field-label">用户名</div>' +
            '<input class="text-input" value="' + escHtml(globalSettings.email_username) + '" ' +
              'onchange="updateGlobalSetting(\'email_username\', this.value)" placeholder="your-email@gmail.com">' +
          '</div>' +
          '<div class="field-group">' +
            '<div class="field-label">密码 <span>(建议使用应用专用密码)</span></div>' +
            '<div class="password-input-group">' +
              '<input class="text-input password-input" type="password" id="email-password" value="' + escHtml(globalSettings.email_password) + '" ' +
                'onchange="updateGlobalSetting(\'email_password\', this.value)" placeholder="••••••••" autocomplete="off">' +
              '<button class="btn-icon password-toggle" onclick="togglePasswordVisibility(\'email-password\')" title="显示/隐藏密码">' +
                '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>' +
              '</button>' +
            '</div>' +
          '</div>' +
        '</div>' +
        // 第三行：批量和限流
        '<div class="field-row">' +
          '<div class="field-group">' +
            '<div class="field-label">批量大小 <span>(0 = 每个事件单独发送)</span></div>' +
            '<input class="text-input" type="number" value="' + globalSettings.email_batch_size + '" ' +
              'onchange="updateGlobalSetting(\'email_batch_size\', parseInt(this.value))" min="0">' +
          '</div>' +
          '<div class="field-group">' +
            '<div class="field-label">每分钟上限</div>' +
            '<input class="text-input" type="number" value="' + globalSettings.email_max_per_minute + '" ' +
              'onchange="updateGlobalSetting(\'email_max_per_minute\', parseInt(this.value))" min="1">' +
          '</div>' +
        '</div>' +
      '</div>' +
    '</div>' +

    // ── Syslog 通知 ──
    '<div class="settings-section">' +
      '<div class="section-title">Syslog 通知</div>' +

      // Syslog 开关
      '<div class="field-group">' +
        '<label class="toggle"><input type="checkbox" ' + (globalSettings.syslog_enabled ? 'checked' : '') +
          ' onchange="updateGlobalSetting(\'syslog_enabled\', this.checked)">' +
          '<span class="toggle-slider"></span></label>' +
        '<span class="toggle-label">启用 Syslog</span>' +
      '</div>' +

      // Syslog 设置详情（仅在启用时显示）
      '<div id="syslog-settings-details" style="' + (globalSettings.syslog_enabled ? '' : 'display:none') + '">' +
        '<div class="field-row">' +
          '<div class="field-group">' +
            '<div class="field-label">Syslog 服务器</div>' +
            '<input class="text-input" value="' + escHtml(globalSettings.syslog_server) + '" ' +
              'onchange="updateGlobalSetting(\'syslog_server\', this.value)" placeholder="localhost">' +
          '</div>' +
          '<div class="field-group">' +
            '<div class="field-label">端口</div>' +
            '<input class="text-input" type="number" value="' + globalSettings.syslog_port + '" ' +
              'onchange="updateGlobalSetting(\'syslog_port\', parseInt(this.value))" placeholder="514">' +
          '</div>' +
          '<div class="field-group">' +
            '<div class="field-label">协议格式</div>' +
            '<select class="select-input" onchange="updateGlobalSetting(\'syslog_format\', this.value)">' +
              '<option value="rfc3164"' + (globalSettings.syslog_format === 'rfc3164' ? ' selected' : '') + '>rfc3164 (BSD Syslog)</option>' +
              '<option value="rfc5424"' + (globalSettings.syslog_format === 'rfc5424' ? ' selected' : '') + '>rfc5424 (现代 Syslog)</option>' +
            '</select>' +
          '</div>' +
        '</div>' +
      '</div>' +
    '</div>' +

    // 保存/重置按钮
    '<div class="card-actions">' +
      '<button class="btn-save" id="btn-save-global" ' + (globalDirty ? '' : 'disabled') + ' onclick="saveGlobalSettings()">' +
        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11l5 5v11a2 2 0 0 1-2 2z"/><polyline points="17 21 17 13 7 13 7 21"/><polyline points="7 3 7 8 15 8"/></svg>' +
        '保存' +
      '</button>' +
      '<button class="btn-reset" id="btn-reset-global" ' + (globalDirty ? '' : 'disabled') + ' onclick="resetGlobalSettings()">' +
        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/></svg>' +
        '重置' +
      '</button>' +
      '<div class="save-status" id="save-status-global">' + ICONS.check + ' 已保存到配置文件</div>' +
    '</div>' +
  '</div>';

  settingsPanel.innerHTML = html;
}

async function updateWatch(idx, field, value) {
  watches[idx][field] = value;
  markDirty(idx);
}

function toggleEventType(idx, checkbox) {
  const t = checkbox.dataset.type;
  const label = checkbox.parentElement;
  if (checkbox.checked) {
    label.classList.add('checked');
    // 使用规范化函数检查是否已存在
    if (!watches[idx].event_types.some(e => eventTypeMatches(e, t))) {
      watches[idx].event_types.push(t);
    }
  } else {
    label.classList.remove('checked');
    // 如果当前是空数组（全选状态），先展开为完整列表再移除
    if (watches[idx].event_types.length === 0) {
      watches[idx].event_types = EVENT_TYPES.filter(x => x !== t);
    } else {
      // 使用规范化函数过滤
      watches[idx].event_types = watches[idx].event_types.filter(x => !eventTypeMatches(x, t));
    }
  }
  // 如果全部选中，压缩为空数组（表示全选）
  if (watches[idx].event_types.length === EVENT_TYPES.length) {
    watches[idx].event_types = [];
  }
  markDirty(idx);
}

function addTagKey(e, idx, field) {
  if (e.key !== 'Enter') return;
  const val = e.target.value.trim();
  if (!val) return;
  if (!watches[idx][field].includes(val)) {
    watches[idx][field].push(val);
  }
  e.target.value = '';
  markDirty(idx);
  renderSettings();
}

function removeTag(idx, field, value) {
  watches[idx][field] = watches[idx][field].filter(x => x !== value);
  markDirty(idx);
  renderSettings();
}

// ── Dirty State Management ──────────────────────────
function markDirty(idx) {
  dirtyFlags[idx] = true;
  // 更新 UI 指示器
  const dirtyEl = document.getElementById('dirty-' + idx);
  if (dirtyEl) dirtyEl.classList.add('show');
  const saveBtn = document.getElementById('btn-save-' + idx);
  const resetBtn = document.getElementById('btn-reset-' + idx);
  if (saveBtn) saveBtn.disabled = false;
  if (resetBtn) resetBtn.disabled = false;
}

function markClean(idx) {
  dirtyFlags[idx] = false;
  const dirtyEl = document.getElementById('dirty-' + idx);
  if (dirtyEl) dirtyEl.classList.remove('show');
  const saveBtn = document.getElementById('btn-save-' + idx);
  const resetBtn = document.getElementById('btn-reset-' + idx);
  if (saveBtn) saveBtn.disabled = true;
  if (resetBtn) resetBtn.disabled = true;
}

// ── Save/Reset Card ─────────────────────────────────
async function saveCard(idx) {
  const w = watches[idx];
  const isNew = newFlags[idx] || false;
  try {
    let resp;
    if (isNew) {
      // New watcher: POST to create
      resp = await fetch('/api/config/watches', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          path: w.path,
          recursive: w.recursive,
          include: w.include,
          exclude: w.exclude,
          event_types: w.event_types,
        })
      });
      if (resp.ok) {
        newFlags[idx] = false;  // No longer new
      } else if (resp.status === 409) {
        showMessageModal('保存失败', '该路径已存在监控目标');
        return;
      }
    } else {
      // Existing watcher: PUT to update
      resp = await fetch('/api/config/watches/' + idx, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', ...authHeaders() },
        body: JSON.stringify({
          recursive: w.recursive,
          include: w.include,
          exclude: w.exclude,
          event_types: w.event_types,
        })
      });
    }
    if (resp.ok) {
      // 保存成功，更新原始状态
      originalWatches[idx] = JSON.parse(JSON.stringify(watches[idx]));
      markClean(idx);
      setPendingReload();
      renderSettings();  // Re-render to remove "new" indicator
      const status = document.getElementById('save-status-' + idx);
      if (status) {
        status.innerHTML = ICONS.check + ' 已保存到配置文件（需重载生效）';
        status.classList.add('show');
        setTimeout(() => status.classList.remove('show'), 3500);
      }
    } else {
      showMessageModal('保存失败', '服务器返回错误: ' + resp.statusText);
    }
  } catch (e) {
    showMessageModal('保存失败', '连接服务器失败: ' + e.message);
  }
}

// ── Watch Management ──────────────────────────────────
function showAddWatchDialog() {
  showPromptModal('添加监控目标', '请输入要监控的目录路径', (path) => {
    addWatch(path);
  });
}

async function reloadWatchers() {
  try {
    const resp = await fetch('/api/watchers/reload', {
      method: 'POST',
      headers: authHeaders()
    });
    if (resp.ok) {
      const data = await resp.json();
      if (data.ok) {
        clearPendingReload();
        showMessageModal('重载成功',
          '监控配置已重新加载<br><br>' +
          '<strong>新增：</strong>' + (data.added.length || '无') + '<br>' +
          '<strong>移除：</strong>' + (data.removed.length || '无') + '<br>' +
          '<strong>保持：</strong>' + (data.kept.length || '无')
        );
        // Reload config to refresh UI
        loadConfig();
      } else {
        showMessageModal('重载失败', data.error || '未知错误');
      }
    } else {
      showMessageModal('重载失败', '服务器返回错误: ' + resp.statusText);
    }
  } catch (e) {
    showMessageModal('重载失败', '连接服务器失败: ' + e.message);
  }
}

async function addWatch(path) {
  // Check duplicate in local state
  if (watches.some(w => w.path === path)) {
    showMessageModal('添加失败', '该路径已存在监控目标');
    return;
  }
  // Only add to local state, not persisted until user clicks save
  const newWatch = {
    path: path,
    recursive: true,
    include: [],
    exclude: [],
    event_types: [],
  };
  watches.push(newWatch);
  originalWatches.push(JSON.parse(JSON.stringify(newWatch)));
  dirtyFlags.push(true);  // New watchers are always dirty
  newFlags.push(true);    // Mark as new (not yet saved to backend)
  renderSettings();
  setPendingReload();
}

function deleteWatch(idx) {
  const w = watches[idx];
  const isNew = newFlags[idx] || false;

  if (isNew) {
    // Unsaved watcher: just remove from local state
    watches.splice(idx, 1);
    originalWatches.splice(idx, 1);
    dirtyFlags.splice(idx, 1);
    newFlags.splice(idx, 1);
    renderSettings();
    return;
  }

  showConfirmModal(
    '删除监控目标',
    '确定要删除以下监控目标？<br><br><strong>路径：</strong>' + escHtml(w.path),
    async () => {
      try {
        const resp = await fetch('/api/config/watches/' + idx, {
          method: 'DELETE',
          headers: authHeaders()
        });
        if (resp.ok) {
          watches.splice(idx, 1);
          originalWatches.splice(idx, 1);
          dirtyFlags.splice(idx, 1);
          newFlags.splice(idx, 1);
          setPendingReload();
          renderSettings();
        } else {
          showMessageModal('删除失败', '服务器返回错误: ' + resp.statusText);
        }
      } catch (e) {
        showMessageModal('删除失败', '连接服务器失败: ' + e.message);
      }
    },
    true
  );
}

function resetCard(idx) {
  if (newFlags[idx]) {
    // New watcher: remove entirely on reset
    watches.splice(idx, 1);
    originalWatches.splice(idx, 1);
    dirtyFlags.splice(idx, 1);
    newFlags.splice(idx, 1);
  } else {
    // Existing watcher: restore from original state
    watches[idx] = JSON.parse(JSON.stringify(originalWatches[idx]));
    markClean(idx);
  }
  renderSettings();
}

// ── Global Settings ──────────────────────────────────
function updateGlobalSetting(field, value) {
  globalSettings[field] = value;

  // 特殊处理：数据库启用时显示路径选项
  if (field === 'database_enabled') {
    const pathGroup = document.getElementById('database-path-group');
    if (pathGroup) pathGroup.style.display = value ? '' : 'none';
  }

  // 特殊处理：邮件启用时显示详细设置
  if (field === 'email_enabled') {
    const details = document.getElementById('email-settings-details');
    if (details) details.style.display = value ? '' : 'none';
  }

  // 特殊处理：Syslog 启用时显示详细设置
  if (field === 'syslog_enabled') {
    const details = document.getElementById('syslog-settings-details');
    if (details) details.style.display = value ? '' : 'none';
  }

  globalDirty = true;
  const dirtyEl = document.getElementById('dirty-global');
  if (dirtyEl) dirtyEl.classList.add('show');
  const saveBtn = document.getElementById('btn-save-global');
  const resetBtn = document.getElementById('btn-reset-global');
  if (saveBtn) saveBtn.disabled = false;
  if (resetBtn) resetBtn.disabled = false;
}

async function saveGlobalSettings() {
  try {
    const resp = await fetch('/api/config/global', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(globalSettings)
    });
    if (resp.ok) {
      originalGlobalSettings = JSON.parse(JSON.stringify(globalSettings));
      globalDirty = false;
      const dirtyEl = document.getElementById('dirty-global');
      if (dirtyEl) dirtyEl.classList.remove('show');
      const saveBtn = document.getElementById('btn-save-global');
      const resetBtn = document.getElementById('btn-reset-global');
      if (saveBtn) saveBtn.disabled = true;
      if (resetBtn) resetBtn.disabled = true;
      const status = document.getElementById('save-status-global');
      if (status) { status.classList.add('show'); setTimeout(() => status.classList.remove('show'), 2500); }
    } else {
      alert('保存失败: ' + resp.statusText);
    }
  } catch (e) {
    alert('保存失败: ' + e.message);
  }
}

function resetGlobalSettings() {
  globalSettings = JSON.parse(JSON.stringify(originalGlobalSettings));
  globalDirty = false;
  renderSettings();
}

// ── Password Toggle ──────────────────────────────────
function togglePasswordVisibility(inputId) {
  const input = document.getElementById(inputId);
  if (input) {
    input.type = input.type === 'password' ? 'text' : 'password';
  }
}
