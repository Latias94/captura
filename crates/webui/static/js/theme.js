(function(){
  function apply(){
    const m = document.cookie.match(/(?:^|; )theme=([^;]+)/);
    const theme = m ? decodeURIComponent(m[1]) : 'system';
    const root = document.documentElement;
    if(theme==='light'){ root.setAttribute('data-theme','light'); }
    else if(theme==='dark'){ root.setAttribute('data-theme','dark'); }
    else { root.removeAttribute('data-theme'); }
    // compact/minimal
    const compact = /(?:^|; )compact_ui=1/.test(document.cookie);
    const minimal = /(?:^|; )minimal_ui=1/.test(document.cookie);
    root.setAttribute('data-compact', compact ? '1' : '0');
    root.setAttribute('data-minimal', minimal ? '1' : '0');
  }
  if(document.readyState==='loading') document.addEventListener('DOMContentLoaded', apply); else apply();
})();
