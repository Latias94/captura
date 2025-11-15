// Simple login helper without inline scripts (CSP-friendly)
(function(){
  const btn = document.getElementById('loginBtn');
  const useBtn = document.getElementById('useBtn');
  const form = document.getElementById('loginForm');
  if(!form) return;
  const T = {
    failed: form.dataset.msgFailed || 'Login failed',
    network: form.dataset.msgNetwork || 'Network error',
    issued: form.dataset.msgIssued || 'Token issued. Copy and use X-Auth-Token in requests.'
  };

  async function doLogin(){
    const u = (document.getElementById('username') || {}).value || '';
    const p = (document.getElementById('password') || {}).value || '';
    const msg = document.getElementById('loginMsg');
    if(msg){ msg.textContent = ''; }
    try{
      const resp = await fetch('/api/v1/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: u, password: p })
      });
      if(!resp.ok){
        if(msg){ msg.textContent = T.failed; msg.style.color = '#dc2626'; }
        return;
      }
      const json = await resp.json();
      const token = json.token || '';
      if(msg){ msg.textContent = T.issued; msg.style.color = '#16a34a'; }
      // Show token in a readonly textarea (create on the fly)
      let ta = document.getElementById('tokenBox');
      if(!ta){
        ta = document.createElement('textarea');
        ta.id = 'tokenBox';
        ta.readOnly = true;
        ta.className = 'tokenbox input';
        (btn && btn.parentElement ? btn.parentElement : form).appendChild(ta);
      }
      ta.value = token;
      ta.select();
      if(useBtn){ useBtn.disabled = false; useBtn.dataset.token = token; }

      // Automatically persist token for WebUI and go to feeds.
      document.cookie = 'X-Auth-Token=' + token + '; Path=/; SameSite=Lax';
      window.location.href = '/feeds';
    }catch(e){
      const msg = document.getElementById('loginMsg');
      if(msg){ msg.textContent = T.network; msg.style.color = '#dc2626'; }
    }
  }

  if(btn){ btn.addEventListener('click', doLogin); }
  // Support submitting with Enter: intercept form submission to avoid
  // default browser behavior that may conflict with CSP/encoding.
  form.addEventListener('submit', function(e){ e.preventDefault(); doLogin(); });

  if(useBtn){
    useBtn.addEventListener('click', function(){
      const token = useBtn.dataset.token || '';
      if(!token){ return; }
      document.cookie = 'X-Auth-Token=' + token + '; Path=/; SameSite=Lax';
      window.location.href = '/feeds';
    });
  }
})();
