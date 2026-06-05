// ── Modal Helpers ─────────────────────────────────────
function showModal(title, bodyHtml, actions) {
  const overlay = document.getElementById('modalOverlay');
  document.getElementById('modalTitle').textContent = title;
  document.getElementById('modalBody').innerHTML = bodyHtml;
  document.getElementById('modalActions').innerHTML = actions;
  overlay.classList.add('active');
}

function closeModal() {
  document.getElementById('modalOverlay').classList.remove('active');
}

function showPromptModal(title, placeholder, callback) {
  const bodyHtml = '<input class="modal-input" id="modalInput" placeholder="' + placeholder + '" autofocus>';
  const actions = '<button class="modal-btn modal-btn-cancel" onclick="closeModal()">取消</button>' +
    '<button class="modal-btn modal-btn-confirm" onclick="submitPromptModal()">确定</button>';
  showModal(title, bodyHtml, actions);
  window._modalCallback = callback;
  setTimeout(() => {
    const input = document.getElementById('modalInput');
    if (input) {
      input.focus();
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') submitPromptModal(); });
    }
  }, 100);
}

function submitPromptModal() {
  const input = document.getElementById('modalInput');
  const value = input ? input.value.trim() : '';
  closeModal();
  if (value && window._modalCallback) {
    window._modalCallback(value);
  }
}

function showConfirmModal(title, message, callback, danger) {
  const btnClass = danger ? 'modal-btn-danger' : 'modal-btn-confirm';
  const btnText = danger ? '删除' : '确定';
  const bodyHtml = '<div class="modal-message">' + message + '</div>';
  const actions = '<button class="modal-btn modal-btn-cancel" onclick="closeModal()">取消</button>' +
    '<button class="modal-btn ' + btnClass + '" onclick="closeModal(); window._modalConfirmCallback()">' + btnText + '</button>';
  showModal(title, bodyHtml, actions);
  window._modalConfirmCallback = callback;
}

function showMessageModal(title, message) {
  const bodyHtml = '<div class="modal-message">' + message + '</div>';
  const actions = '<button class="modal-btn modal-btn-confirm" onclick="closeModal()">确定</button>';
  showModal(title, bodyHtml, actions);
}
