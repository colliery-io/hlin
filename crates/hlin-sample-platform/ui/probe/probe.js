// The probe: the smallest module that exercises the parent end of the bridge.
//
// It does what the specification asks of a module and no more: acts on the
// first `init` and ignores the page's resends, says `ready`, echoes every
// `heartbeat`, and asks its platform one question over `fetch`. Two buttons
// make it misbehave on purpose, for the browser tests: one stops it answering
// heartbeats, which is what a wedged module looks like from outside, and one
// sends more requests at once than a frame may have in flight.

(function () {
  'use strict';

  var BRIDGE = [1, 0];
  var sent = 0;
  var initialised = false;
  var answering = true;
  // What each outstanding `fetch` was for, by its id.
  var waiting = {};
  var flood = { answered: 0, refused: 0, other: 0 };

  function send(type, data, re) {
    sent += 1;
    var message = { bridge: BRIDGE, id: 'm-' + sent, type: type, data: data };
    if (re) {
      message.re = re;
    }
    // `*`, because the parent's origin is not something an opaque origin can
    // name, and nothing here is a secret (REQ-2.2).
    window.parent.postMessage(message, '*');
    return message.id;
  }

  function show(id, text) {
    document.getElementById(id).textContent = text;
  }

  function ask(path, purpose) {
    var id = send('fetch', {
      method: 'GET',
      path: path,
      query: '',
      headers: { accept: 'application/json' },
    });
    waiting[id] = purpose;
  }

  function answered(message) {
    var purpose = waiting[message.re];
    if (!purpose) {
      return;
    }
    delete waiting[message.re];
    var data = message.data;

    if (purpose === 'flood') {
      if (data.refusal === 'too_many') {
        flood.refused += 1;
      } else if (!data.refusal && data.status === 200) {
        flood.answered += 1;
      } else {
        flood.other += 1;
      }
      show('flood', flood.answered + ' answered, ' + flood.refused + ' refused, ' + flood.other + ' other');
      return;
    }

    if (data.refusal) {
      show('whoami', 'the shell refused: ' + data.refusal);
      return;
    }
    var text = data.body ? new TextDecoder().decode(data.body) : '';
    try {
      var body = JSON.parse(text);
      show('whoami', body.principal + ' on ' + body.platform + ' (' + data.status + ')');
    } catch (error) {
      show('whoami', 'an unreadable answer (' + data.status + ')');
    }
  }

  window.addEventListener('message', function (event) {
    // Only the page that framed us speaks the bridge.
    if (event.source !== window.parent) {
      return;
    }
    var message = event.data;
    if (!message || typeof message !== 'object' || !message.data) {
      return;
    }

    switch (message.type) {
      case 'init':
        // The page resends `init` until it hears `ready`, because it cannot
        // know when this script started listening. Only the first counts.
        if (initialised) {
          return;
        }
        initialised = true;
        document.body.setAttribute('data-instance', message.data.instance);
        show('viewer', (message.data.viewer && message.data.viewer.name) || 'someone unnamed');
        send('ready', { kit: 'hand-written' });
        document.getElementById('state').setAttribute('data-state', 'ready');
        show('state', 'ready');
        ask('/api/module/whoami', 'whoami');
        break;

      case 'heartbeat':
        if (answering) {
          send('heartbeat', { n: message.data.n }, message.id);
        }
        break;

      case 'response':
        answered(message);
        break;

      default:
        // Anything else, including types from a newer minor, is ignored.
        break;
    }
  });

  document.getElementById('stop').addEventListener('click', function () {
    answering = false;
    document.getElementById('state').setAttribute('data-state', 'silent');
    show('state', 'no longer answering heartbeats');
  });

  document.getElementById('twenty').addEventListener('click', function () {
    flood = { answered: 0, refused: 0, other: 0 };
    for (var i = 0; i < 20; i += 1) {
      ask('/api/module/whoami', 'flood');
    }
  });
})();
