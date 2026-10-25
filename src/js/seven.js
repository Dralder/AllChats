(function () {
  if (window.__ac7Config) return;
  var on = false;
  var map = null;
  var asked = false;
  var observer = null;
  var styled = false;
  var css = 'img.ac7{height:1.6em;width:auto;vertical-align:middle;display:inline-block;margin:0 1px}';
  var CHAT_SELECTORS = [
    ".leading-\\[1\\.55\\].font-normal",
    ".chat-line-message-body",
    ".style-scope.yt-live-chat-text-message-renderer",
    ".chat-scrollable-area__message-container",
    ".chat-line__message",
    ".link-fragment",
    "#seventv-message-container",
    ".seventv-chat-list",
    ".seventv-message",
    ".seventv-chat-message-body",
    ".seventv-painted-content",
    ".seventv-chat-badge",
    ".seventv-overlay",
    "#chatroom",
    "#chatroom-messages",
    ".chatroom-banner",
    ".chat-entry-content",
    ".chat-entry-username",
    ".chat-emote-container",
    ".break-all",
    "seventv-container",
    ".seventv-container",
    "#message-input",
    ".seventv-emote-menu-providers",
    ".seventv-emote-menu-provider-icon",
    ".seventv-emote-menu-search",
    ".seventv-emote-menu-search-input",
    ".seventv-emote-set-container",
    ".seventv-set-header",
    ".seventv-set-header-icon",
    ".seventv-tooltip-compact",
    ".seventv-transparent"
  ];
  var CHAT_SEL = '';

  function ready(fn) {
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', fn, { once: true });
    else fn();
  }

  function has(name) {
    return Object.prototype.hasOwnProperty.call(map, name);
  }

  function skipNode(n) {
    var p = n.parentElement;
    if (!p) return true;
    var t = p.tagName;
    if (t === 'SCRIPT' || t === 'STYLE' || t === 'TEXTAREA' || t === 'INPUT' || t === 'NOSCRIPT') return true;
    if (p.isContentEditable) return true;
    if (CHAT_SEL && !p.closest(CHAT_SEL)) return true;
    return false;
  }

  function convert(node) {
    var txt = node.nodeValue;
    if (!txt || txt === '' || !node.parentNode) return;
    var parts = txt.split(/(\s+)/);
    var hit = false;
    for (var i = 0; i < parts.length; i++) {
      if (parts[i] && has(parts[i])) { hit = true; break; }
    }
    if (!hit) return;
    var frag = document.createDocumentFragment();
    var buf = '';
    parts.forEach(function (p) {
      if (p && has(p)) {
        if (buf) { frag.appendChild(document.createTextNode(buf)); buf = ''; }
        var img = document.createElement('img');
        img.className = 'ac7';
        img.src = map[p];
        img.alt = p;
        img.setAttribute('data-ac7', p);
        img.onerror = function () {
          var txt = this.getAttribute('data-ac7');
          if (txt && this.parentNode) this.parentNode.replaceChild(document.createTextNode(txt), this);
        };
        frag.appendChild(img);
      } else {
        buf += p;
      }
    });
    if (buf) frag.appendChild(document.createTextNode(buf));
    node.parentNode.replaceChild(frag, node);
  }

  function walk(root) {
    if (!on || !map || !root) return;
    if (root.nodeType === 3) {
      if (!skipNode(root)) convert(root);
      return;
    }
    if (root.nodeType !== 1) return;
    if (CHAT_SEL && !root.closest(CHAT_SEL) && !root.querySelector(CHAT_SEL)) return;
    var w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    var list = [];
    while (w.nextNode()) list.push(w.currentNode);
    list.forEach(function (n) { if (!skipNode(n)) convert(n); });
  }

  function revert() {
    Array.prototype.forEach.call(document.querySelectorAll('img.ac7'), function (i) {
      i.replaceWith(document.createTextNode(i.getAttribute('data-ac7') || i.alt));
    });
  }

  function post(data) {
    try {
      window.ipc.postMessage(JSON.stringify(Object.assign({ t: '7tv_id' }, data)));
    } catch (e) {}
  }

  function detect() {
    if (asked) return;
    asked = true;
    var h = location.hostname;
    if (/(^|\.)kick\.com$/.test(h)) {
      var m = location.pathname.match(/^\/popout\/([^\/]+)\/chat/) || location.pathname.match(/^\/([^\/?#]+)/);
      var slug = m ? m[1] : null;
      if (!slug) { post(null); return; }
      fetch('/api/v2/channels/' + encodeURIComponent(slug), { credentials: 'include' })
        .then(function (r) { return r.json(); })
        .then(function (j) {
          var id = j && (j.user_id || (j.user && j.user.id));
          post(id ? { platform: 'kick', id: id } : null);
        })
        .catch(function () { post(null); });
    } else if (/(^|\.)twitch\.tv$/.test(h)) {
      var reserved = { directory: 1, videos: 1, settings: 1, downloads: 1, jobs: 1, turbo: 1, search: 1, p: 1, popout: 1, u: 1, subscriptions: 1, wallet: 1, friends: 1, drops: 1, prime: 1 };
      var m = location.pathname.match(/^\/(?:popout|embed)\/([a-z0-9_]{1,25})\/chat/i) || location.pathname.match(/^\/([a-z0-9_]{1,25})(?:\/chat)?\/?$/i);
      var login = m ? m[1].toLowerCase() : null;
      if (login && reserved[login]) login = null;
      post(login ? { platform: 'twitch', login: login } : null);
    } else if (/(^|\.)youtube\.com$/.test(h)) {
      var m = location.pathname.match(/^\/channel\/(UC[\w-]{22})/);
      if (m) {
        post({ platform: 'youtube', id: m[1] });
      } else {
        var params = new URLSearchParams(location.search);
        var v = params.get('v');
        if (v) {
          fetch('/watch?v=' + encodeURIComponent(v), { credentials: 'include' })
            .then(function (r) { return r.text(); })
            .then(function (t) {
              var m = t.match(/"videoDetails":[\s\S]{0,3000}?"channelId":"(UC[\w-]{22})"/);
              if (m) post({ platform: 'youtube', id: m[1] });
              else post(null);
            })
            .catch(function () { post(null); });
        } else if (location.pathname.indexOf('/@') === 0) {
          fetch(location.pathname, { credentials: 'include' })
            .then(function (r) { return r.text(); })
            .then(function (t) {
              var m = t.match(/"channelId":"(UC[\w-]{22})"/);
              if (m) post({ platform: 'youtube', id: m[1] });
              else post(null);
            })
            .catch(function () { post(null); });
        } else {
          post(null);
        }
      }
    } else {
      post(null);
    }
  }

  function start() {
    ready(function () {
      if (!CHAT_SEL) {
        var valid = [];
        var frag = document.createDocumentFragment();
        CHAT_SELECTORS.forEach(function (sel) {
          try {
            frag.querySelector(sel);
            valid.push(sel);
          } catch (e) {}
        });
        CHAT_SEL = valid.join(',');
      }
      if (!styled && document.documentElement) {
        var st = document.createElement('style');
        st.textContent = css;
        document.documentElement.appendChild(st);
        styled = true;
      }
      if (!observer && document.body) {
        observer = new MutationObserver(function (ms) {
          if (!on || !map) return;
          ms.forEach(function (m) {
            Array.prototype.forEach.call(m.addedNodes, walk);
          });
        });
        observer.observe(document.body, { childList: true, subtree: true });
      }
      if (map) walk(document.body);
      else detect();
    });
  }

  window.__ac7Config = function (v) {
    v = !!v;
    if (v === on) return;
    on = v;
    if (on) start();
    else revert();
  };

  window.__ac7tv = function (m) {
    map = m || {};
    if (on && document.body) walk(document.body);
  };
})();
