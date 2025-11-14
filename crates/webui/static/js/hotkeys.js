// Minimal hotkeys for entry page: s = star/unstar, m = toggle read
(function(){
  window.addEventListener('keydown', function(e){
    if (e.defaultPrevented) return;
    const tag = (e.target && e.target.tagName || '').toLowerCase();
    if (tag === 'input' || tag === 'textarea') return;
    // Shift+S = Save entry; s = toggle star
    if (e.key === 'S' && e.shiftKey) {
      const btn = document.getElementById('btnSaveEntry');
      if (btn) { btn.click(); e.preventDefault(); return; }
    } else if (e.key === 's') {
      const btn = document.getElementById('btnStar');
      if (btn) { btn.click(); e.preventDefault(); return; }
    } else if (e.key === 'm' || e.key === 'M') {
      const btn = document.getElementById('btnMark');
      if (btn) { btn.click(); e.preventDefault(); }
    } else if (e.key === 'U' && e.shiftKey) {
      const btn = document.getElementById('btnKeepUnread');
      if (btn) { btn.click(); e.preventDefault(); }
    } else if (e.key === 'L' && e.shiftKey) {
      const btn = document.getElementById('btnLoadFull');
      if (btn) { btn.click(); e.preventDefault(); }
    } else if (e.key === 'j' || e.key === 'J') {
      const next = document.getElementById('btnNextNav');
      const root = document.getElementById('entryRoot');
      if (next && root) {
        // auto mark read before navigating
        const id = root.dataset.entryId;
        fetch('/ui/entries/' + id + '/mark', { method: 'POST', headers: {'x-status':'read'} }).finally(()=>{
          next.click();
        });
        e.preventDefault();
      }
    } else if (e.key === 'k' || e.key === 'K') {
      const prev = document.getElementById('btnPrevNav');
      if (prev) { prev.click(); e.preventDefault(); }
    }
  }, {passive: false});
})();
