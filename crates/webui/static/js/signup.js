// Simple signup helper without inline scripts (CSP-friendly)
(function(){
  const btn = document.getElementById('signupBtn');
  const form = document.getElementById('signupForm');
  if (!form || !btn) return;
  const T = {
    failed: form.dataset.msgFailed || 'Signup failed',
    network: form.dataset.msgNetwork || 'Network error',
    created: form.dataset.msgCreated || 'Account created. You can now sign in.',
    disabled: form.dataset.msgDisabled || 'Signup is disabled.',
  };

  async function doSignup(){
    const u = (document.getElementById('signup_username') || {}).value || '';
    const p = (document.getElementById('signup_password') || {}).value || '';
    const msg = document.getElementById('signupMsg');
    if (msg) { msg.textContent = ''; msg.style.color = ''; }
    if (!u.trim() || !p) {
      if (msg) {
        msg.textContent = T.failed;
        msg.style.color = '#dc2626';
      }
      return;
    }

    try {
      const resp = await fetch('/api/v1/users', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: u, password: p }),
      });
      if (!resp.ok) {
        let detail = T.failed;
        if (resp.status === 403) {
          detail = T.disabled;
        }
        if (msg) {
          msg.textContent = detail;
          msg.style.color = '#dc2626';
        }
        return;
      }
      if (msg) {
        msg.textContent = T.created;
        msg.style.color = '#16a34a';
      }
      // redirect to login after a short delay
      setTimeout(function(){
        window.location.href = '/login';
      }, 1200);
    } catch (e) {
      const msg = document.getElementById('signupMsg');
      if (msg) {
        msg.textContent = T.network;
        msg.style.color = '#dc2626';
      }
    }
  }

  btn.addEventListener('click', doSignup);
  form.addEventListener('submit', function(e){
    e.preventDefault();
    doSignup();
  });
})();

