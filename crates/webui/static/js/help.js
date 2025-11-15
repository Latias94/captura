(function(){
  const el = document.getElementById('kbHelp');
  const close = document.getElementById('kbClose');
  if (!el) return;

  function show(v){
    if (v) {
      el.removeAttribute('hidden');
      el.setAttribute('aria-hidden','false');
    } else {
      el.setAttribute('hidden','');
      el.setAttribute('aria-hidden','true');
    }
  }

  // Toggle with '?', close with Escape
  window.addEventListener('keydown', function(e){
    const tag = (e.target && e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea') return;
    if (e.key === '?') {
      show(el.hasAttribute('hidden'));
      e.preventDefault();
    }
    if (e.key === 'Escape') {
      show(false);
    }
  }, {passive:false});

  // Click close button
  if (close) {
    close.addEventListener('click', function(){
      show(false);
    });
  }

  // Click outside panel closes; also treat clicks on the close button as a fallback.
  el.addEventListener('click', function(e){
    if (e.target === el) {
      show(false);
      return;
    }
    const t = e.target;
    if (t && t.id === 'kbClose') {
      show(false);
    }
  });
})();
