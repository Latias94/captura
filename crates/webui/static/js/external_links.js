(function(){
  function hasPref(){ return document.cookie.indexOf('open_ext_newtab=1') !== -1; }
  function apply(){
    if(!hasPref()) return;
    // original link
    document.querySelectorAll('a.ext').forEach(a=>{ a.setAttribute('target','_blank'); a.setAttribute('rel','noopener noreferrer'); });
    // article content links
    document.querySelectorAll('.article__content a[href^="http"]').forEach(a=>{ a.setAttribute('target','_blank'); a.setAttribute('rel','noopener noreferrer'); });
  }
  if(document.readyState === 'loading') document.addEventListener('DOMContentLoaded', apply); else apply();
})();

