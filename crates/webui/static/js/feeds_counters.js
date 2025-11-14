(function(){
  function getToken(){
    const m = document.cookie.match(/(?:^|; )X-Auth-Token=([^;]+)/);
    return m ? decodeURIComponent(m[1]) : '';
  }
  async function fetchCounters(){
    const token = getToken();
    if(!token) return null;
    try{
      const resp = await fetch('/v1/feeds/counters', { headers: { 'X-Auth-Token': token }});
      if(!resp.ok) return null;
      return await resp.json();
    }catch(_){ return null; }
  }
  function updateNavTotal(json){
    const nav = document.getElementById('navUnread');
    if(!nav || !json) return;
    const unreads = json.unreads || {};
    let total = 0; for(const k in unreads){ if(Object.prototype.hasOwnProperty.call(unreads,k)) total += unreads[k] || 0; }
    nav.textContent = total;
    if(total > 0) nav.removeAttribute('hidden'); else nav.setAttribute('hidden','');
    // Update viewCategory options counts if present
    const sel = document.getElementById('viewCategory');
    if(sel){
      // First option = All
      const first = sel.querySelector('option[value=""]');
      if(first){
        const base = first.getAttribute('data-title') || first.textContent;
        const totalFeeds = document.querySelectorAll('#feedsList [data-feed-id]').length;
        first.textContent = `${base} (${totalFeeds} / ${total})`;
      }
      // Other options: category_id based
      const options = Array.from(sel.querySelectorAll('option[value]:not([value=""])'));
      options.forEach(opt => {
        const id = opt.value;
        const base = opt.getAttribute('data-title') || opt.textContent;
        // Summation per category: we have cat badges aggregated in updateFeedsList,
        // but here recompute via DOM to avoid another API: sum unreads for feeds within cat
        // if DOM groups exist, prefer group badge, else fallback to 0
        const badge = document.getElementById('catBadge-' + id);
        const n = badge ? parseInt(badge.textContent || '0', 10) || 0 : 0;
        const feeds = opt.getAttribute('data-feeds') || '';
        const k = feeds ? parseInt(feeds,10)||0 : 0;
        opt.textContent = `${base} (${k} / ${n})`;
      });
    }
  }
  function updateFeedsList(json){
    const list = document.getElementById('feedsList');
    if(!list || !json) return;
    const unreads = json.unreads || {};
    const items = Array.from(list.querySelectorAll('[data-feed-id]'));
    const catTotals = {};
    items.forEach(li => {
      const id = li.dataset.feedId;
      const cat = (li.dataset.catId || '');
      const n = unreads[id] || 0;
      const badge = document.getElementById('feedBadge-' + id);
      if(badge){
        badge.textContent = n;
        if(n > 0) badge.removeAttribute('hidden'); else badge.setAttribute('hidden','');
      }
      if(cat){ catTotals[cat] = (catTotals[cat] || 0) + n; }
    });
    // category badges
    Object.keys(catTotals).forEach(cat => {
      const badge = document.getElementById('catBadge-' + cat);
      if(badge){
        const n = catTotals[cat] || 0;
        badge.textContent = n;
        if(n > 0) badge.removeAttribute('hidden'); else badge.setAttribute('hidden','');
      }
    });
    // optional: handle feeds not present in DOM (ignore)
  }
  async function tick(){
    const json = await fetchCounters();
    if(json){ updateNavTotal(json); updateFeedsList(json); }
  }
  // expose manual tick
  window.feedsCountersTick = tick;
  // initial + schedule
  if(document.readyState === 'loading') document.addEventListener('DOMContentLoaded', tick);
  else tick();
  let timer = null;
  function start(){ if(timer) return; timer = setInterval(tick, 15000); }
  function stop(){ if(timer){ clearInterval(timer); timer = null; } }
  document.addEventListener('visibilitychange', function(){ if(document.visibilityState === 'visible') { tick(); start(); } else { stop(); } });
  start();
})();
