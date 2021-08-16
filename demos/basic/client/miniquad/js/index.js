// Implementing auto reloading via websockets
const wsURL = (window.location.protocol=='https:' && 'wss://' || 'ws://') + window.location.host + '/livereload/';
const ws = new WebSocket(wsURL);
ws.onclose = function() {
    setInterval(function() {
        const temp_ws = new WebSocket(wsURL);
        temp_ws.onopen = function() {
            location.reload();
        };
    }, 500);
};