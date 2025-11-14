(function(){
  // Remember open/closed state for category groups
  const key = 'captura:feed-groups-open';
  function load(){ try{ return JSON.parse(localStorage.getItem(key) || '{}') || {}; }catch(_){ return {}; } }
  function save(map){ try{ localStorage.setItem(key, JSON.stringify(map)); }catch(_){ /* ignore */ } }
  function apply(){
    const map = load();
    const groups = document.querySelectorAll('details.panel[data-cat-id]');
    groups.forEach(d => {
      const id = d.getAttribute('data-cat-id'); if(!id) return;
      const want = map[id];
      if(typeof want === 'boolean'){
        if(want) d.setAttribute('open',''); else d.removeAttribute('open');
      }
      d.addEventListener('toggle', function(){
        const m = load(); m[id] = d.hasAttribute('open'); save(m);
      });
    });
  }
  if(document.readyState === 'loading') document.addEventListener('DOMContentLoaded', apply); else apply();
})();

