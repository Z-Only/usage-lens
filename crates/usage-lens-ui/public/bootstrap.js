import init from './usage_lens_ui.js';
init().catch(() => {
  const message = document.createElement('p');
  message.setAttribute('role', 'alert');
  message.textContent = 'The local dashboard could not start. Reload to retry.';
  document.body.append(message);
});
