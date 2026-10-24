(function () {
  var U = window.__acU;
  var hidden = __HIDDEN__;
  var bgs = __BGS__;
  var opacity = __OPACITY__;
  var showFrame = !__HIDE__;
  var style = document.createElement('style');
  var host = null;
  var shadow = null;
  var scrollEl = null;
  var scrollPending = false;
  var autoScrollActive = false;
  var lastFind = 0;
  var findFails = 0;
  var connHost = null;
  var connBox = null;
  var connTimer = null;
  var connLast = null;
  var CONN_SIZE = 24;
  var netOn = false;
  var netTimer = null;
  var netFails = 0;
  var netBusy = false;

  function zoom() {
    return window.__acZoom || 1;
  }

  function netCheck(forced) {
    if (!netOn || netBusy) return;
    if (location.protocol !== 'http:' && location.protocol !== 'https:') return;
    netBusy = true;
    var controller = new AbortController();
    var timedOut = false;
    var abortTimer = setTimeout(function () { timedOut = true; controller.abort(); }, 2000);
    fetch(location.href, { method: 'HEAD', mode: 'no-cors', cache: 'no-store', credentials: 'omit', signal: controller.signal })
      .then(function () { return true; })
      .catch(function () { return false; })
      .then(function (ok) {
        clearTimeout(abortTimer);
        netBusy = false;
        if (!netOn) return;
        if (ok) {
          netFails = 0;
          window.__acConn(true);
          if (netTimer) clearTimeout(netTimer);
          netTimer = setTimeout(function () { netCheck(false); }, 3000);
        } else {
          var isFastFail = !timedOut;
          if (isFastFail) {
            window.__acConn(false);
            if (netTimer) clearTimeout(netTimer);
            netTimer = setTimeout(function () { netCheck(false); }, 1000);
          } else {
            netFails++;
            if (netFails >= 2 || forced || navigator.onLine === false) {
              window.__acConn(false);
            }
            if (netTimer) clearTimeout(netTimer);
            netTimer = setTimeout(function () { netCheck(false); }, 1000);
          }
        }
      });
  }

  window.addEventListener('offline', function () {
    window.__acConn(false);
    netCheck(true);
  });
  window.addEventListener('online', function () {
    netFails = 0;
    netCheck(false);
  });

  window.__acNetEnable = function (on) {
    netOn = !!on;
    if (netTimer) { clearTimeout(netTimer); netTimer = null; }
    netFails = 0;
    if (netOn) {
      if (navigator.onLine === false) {
        window.__acConn(false);
      }
      netCheck(false);
    } else {
      window.__acConn(null);
    }
  };

  function render() {
    var css = hidden.map(function (i) { return i.sel + '{display:none!important}'; }).join('\n');
    css += '\n' + U.bgCss(bgs);
    css += '\n' + U.baseCss(opacity);
    css += '\n*::-webkit-scrollbar{display:none!important}\nhtml{scrollbar-width:none!important}';
    style.textContent = css;
    U.applyBgDirect(opacity);
  }

  function createFrame() {
    if (!document.documentElement) return;
    var z = zoom();
    var b = 2 / z;
    var s = 24 / z;
    var e = 8 / z;
    var c = 12 / z;
    var btnW = 24 / z;
    var fs = 12 / z;
    host = document.createElement('div');
    host.style.cssText = 'position:fixed!important;inset:0!important;z-index:2147483647!important;pointer-events:none!important;display:' + (showFrame ? 'block' : 'none') + '!important;contain:none!important';
    shadow = host.attachShadow({ mode: 'closed' });
    var css = '.f{position:fixed;inset:0;box-sizing:border-box;border:' + b + 'px solid #7B2CBF;pointer-events:none}' +
      '.d{position:absolute;left:0;right:' + btnW + 'px;top:0;height:' + s + 'px;background:rgba(123,44,191,0.35);pointer-events:auto;cursor:move;display:flex;align-items:center;padding-left:' + (8 / z) + 'px}' +
      '.lbl{color:#E0E0E0;font:' + fs + 'px Consolas,monospace;pointer-events:none;user-select:none;-webkit-user-select:none}' +
      '.btn{position:absolute;right:0;top:0;width:' + btnW + 'px;height:' + s + 'px;background:#B83232;color:#fff;font:bold ' + (14 / z) + 'px Consolas,monospace;line-height:' + s + 'px;text-align:center;cursor:pointer;pointer-events:auto}' +
      '.btn:hover{background:#D04040}' +
      '.r{position:absolute;background:transparent;pointer-events:auto}' +
      '.rnw{left:0;top:0;width:' + c + 'px;height:' + c + 'px;background:#7B2CBF;cursor:nw-resize}' +
      '.rn{left:' + c + 'px;right:' + c + 'px;top:0;height:' + e + 'px;cursor:n-resize}' +
      '.rne{right:0;top:0;width:' + c + 'px;height:' + c + 'px;background:#7B2CBF;cursor:ne-resize}' +
      '.rw{left:0;top:' + c + 'px;width:' + e + 'px;bottom:' + c + 'px;cursor:w-resize}' +
      '.re{right:0;top:' + c + 'px;width:' + e + 'px;bottom:' + c + 'px;cursor:e-resize}' +
      '.rsw{left:0;bottom:0;width:' + c + 'px;height:' + c + 'px;background:#7B2CBF;cursor:sw-resize}' +
      '.rs{left:' + c + 'px;right:' + c + 'px;bottom:0;height:' + e + 'px;cursor:s-resize}' +
      '.rse{right:0;bottom:0;width:' + c + 'px;height:' + c + 'px;background:#7B2CBF;cursor:se-resize}';
    shadow.innerHTML = '<style>' + css + '</style>' +
      '<div class="f"></div><div class="d" data-op="drag"><span class="lbl">Overlay</span></div><div class="btn" data-op="close">X</div>' +
      '<div class="r rnw" data-dir="NW"></div><div class="r rn" data-dir="N"></div><div class="r rne" data-dir="NE"></div>' +
      '<div class="r rw" data-dir="W"></div><div class="r re" data-dir="E"></div>' +
      '<div class="r rsw" data-dir="SW"></div><div class="r rs" data-dir="S"></div><div class="r rse" data-dir="SE"></div>';
    shadow.addEventListener('mousedown', function (ev) {
      var t = ev.target;
      if (!t || !t.getAttribute) return;
      var dir = t.getAttribute('data-dir');
      var op = t.getAttribute('data-op');
      if (op === 'close') {
        ev.preventDefault();
        ev.stopPropagation();
        ev.stopImmediatePropagation();
        window.ipc.postMessage(JSON.stringify({ t: 'win', kind: 'overlay', op: 'close' }));
      } else if (dir) {
        window.ipc.postMessage(JSON.stringify({ t: 'win', kind: 'overlay', op: 'resize', dir: dir }));
      } else if (op === 'drag') {
        window.ipc.postMessage(JSON.stringify({ t: 'win', kind: 'overlay', op: 'drag' }));
      }
    }, true);
    document.documentElement.appendChild(host);
  }

  function rebuildFrame() {
    if (host && host.parentElement) host.parentElement.removeChild(host);
    host = null;
    shadow = null;
    createFrame();
  }

  function ensureConn() {
    if (!document.documentElement) return false;
    var z = zoom();
    if (!connHost) {
      connHost = document.createElement('div');
      connHost.style.cssText = 'position:fixed!important;inset:0!important;z-index:2147483646!important;pointer-events:none!important;display:block!important;contain:none!important';
      var sh = connHost.attachShadow({ mode: 'closed' });
      sh.innerHTML = '<style>.t{position:fixed;font-family:Consolas,monospace;font-weight:bold;pointer-events:none;user-select:none;-webkit-user-select:none;white-space:nowrap;text-shadow:0 0 4px #000,0 0 4px #000,0 1px 3px #000}</style><div class="t"></div>';
      connBox = sh.querySelector('.t');
    }
    connBox.style.left = (20 / z) + 'px';
    connBox.style.bottom = (20 / z) + 'px';
    connBox.style.fontSize = (CONN_SIZE / z) + 'px';
    if (!connHost.parentElement) document.documentElement.appendChild(connHost);
    return true;
  }

  function hideConn() {
    if (connTimer) { clearTimeout(connTimer); connTimer = null; }
    if (connHost && connHost.parentElement) connHost.parentElement.removeChild(connHost);
  }

  window.__acConn = function (v) {
    if (v === connLast) return;
    connLast = v;
    hideConn();
    if (v === null || v === undefined) return;
    if (!ensureConn()) return;
    if (v) {
      connBox.textContent = 'Connected';
      connBox.style.color = '#FFFFFF';
      connTimer = setTimeout(function () { connTimer = null; hideConn(); }, 5000);
    } else {
      connBox.textContent = 'Connection lost';
      connBox.style.color = '#D04040';
    }
  };

  function attachObserver() {
    if (!document.documentElement) return;
    new MutationObserver(function () {
      var root = document.documentElement;
      if (host && !host.parentElement) root.appendChild(host);
      if (connHost && connLast !== null && connLast !== undefined && !connHost.parentElement && (connLast === false || connTimer)) {
        root.appendChild(connHost);
      }
    }).observe(document.documentElement, { childList: true });
  }

  function mount() {
    var root = document.documentElement;
    if (!root) return false;
    if (!style.parentNode) root.appendChild(style);
    if (!host) {
      createFrame();
      attachObserver();
    }
    if (host) {
      host.style.setProperty('display', showFrame ? 'block' : 'none', 'important');
    }
    return true;
  }

  function scrollable(el) {
    return !!el && el.isConnected && el.scrollHeight > el.clientHeight + 1;
  }

  function findScrollEl() {
    if (scrollable(scrollEl)) return scrollEl;
    var now = Date.now();
    if (now - lastFind < (findFails < 3 ? 300 : 2000)) return null;
    lastFind = now;
    scrollEl = null;
    if (!document.body) return null;
    var all = document.body.querySelectorAll('*');
    var best = null;
    var bestScore = 0;
    var vw = window.innerWidth;
    var vh = window.innerHeight;
    var n = Math.min(all.length, 1500);
    for (var i = 0; i < n; i++) {
      var el = all[i];
      if (el.scrollHeight <= el.clientHeight + 1) continue;
      var oy = getComputedStyle(el).overflowY;
      if (oy !== 'auto' && oy !== 'scroll') continue;
      var r = el.getBoundingClientRect();
      if (r.width < vw * 0.5 || r.height < vh * 0.5) continue;
      var score = r.width * r.height;
      if (score > bestScore) {
        best = el;
        bestScore = score;
      }
    }
    scrollEl = best;
    findFails = best ? 0 : findFails + 1;
    return scrollEl;
  }

  function doScroll() {
    scrollPending = false;
    if (!autoScrollActive || document.hidden) return;
    var el = findScrollEl();
    if (el) {
      var target = el.scrollHeight - el.clientHeight;
      if (el.scrollTop < target - 1) el.scrollTop = el.scrollHeight;
      return;
    }
    var de = document.documentElement;
    if (de && de.scrollHeight > window.innerHeight + 1) {
      window.scrollTo(0, de.scrollHeight);
    } else if (document.body && document.body.scrollHeight > window.innerHeight + 1) {
      window.scrollTo(0, document.body.scrollHeight);
    }
  }

  function scheduleScroll() {
    if (!autoScrollActive || scrollPending) return;
    scrollPending = true;
    setTimeout(function () { requestAnimationFrame(doScroll); }, 60);
  }

  function setupAutoScroll(active) {
    autoScrollActive = active;
    if (active) {
      if (!document.__acScrollObs) {
        document.__acScrollObs = new MutationObserver(scheduleScroll);
        document.__acScrollObs.observe(document.documentElement || document, { childList: true, subtree: true });
        window.addEventListener('resize', scheduleScroll);
        window.addEventListener('load', scheduleScroll);
        document.addEventListener('visibilitychange', function () {
          if (!document.hidden) scheduleScroll();
        });
      }
      scheduleScroll();
    }
  }

  window.__acSetZoom = function (z) {
    window.__acZoom = z;
    if (host) rebuildFrame();
    if (connHost && connHost.parentElement) ensureConn();
    scheduleScroll();
  };

  window.__acEdit = function (show) {
    showFrame = !!show;
    if (host) host.style.setProperty('display', showFrame ? 'block' : 'none', 'important');
    setupAutoScroll(!showFrame);
  };

  window.__acApply = function (h, b, op, a, s, edit) {
    hidden = h;
    bgs = b;
    opacity = op;
    showFrame = !!edit;
    render();
    mount();
    U.setAuto(a);
    window.__ac7Config(s);
    setupAutoScroll(!showFrame);
  };

  render();
  if (!mount()) {
    var mo = new MutationObserver(function () { if (mount()) mo.disconnect(); });
    mo.observe(document, { childList: true });
  }
  U.setAuto(__AUTO__);
  window.__ac7Config(__SEVEN__);
  setupAutoScroll(showFrame === false);
})();
