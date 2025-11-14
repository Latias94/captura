(function(){
  const list = document.getElementById('feedsSavedSearches');
  const feedsList = document.getElementById('feedsList');
  if(!list || !feedsList) return;
  const KEY='captura:saved-searches';
  function load(){ try{ return JSON.parse(localStorage.getItem(KEY)||'{}')||{}; }catch(_){ return {}; } }
  const mapTitle = {}; feedsList.querySelectorAll('[data-feed-id]').forEach(li=>{ mapTitle[li.dataset.feedId] = (li.querySelector('a.link')||{}).textContent || li.dataset.feedId; });
  const data = load();
  const feeds = Object.keys(data);
  if(feeds.length===0){ const li=document.createElement('li'); li.className='list__item'; li.textContent=list.dataset.dictEmpty||'No saved searches'; list.appendChild(li); return; }
  feeds.forEach(fid=>{
    const arr = data[fid]||[];
    arr.forEach(it=>{
      const li=document.createElement('li'); li.className='list__item';
      const a=document.createElement('a'); a.className='link'; a.href='/feeds/'+fid+'?q='+encodeURIComponent(it.q); a.textContent=(it.name||it.q)+' — '+(mapTitle[fid]||fid);
      li.appendChild(a); list.appendChild(li);
    });
  });
})();

