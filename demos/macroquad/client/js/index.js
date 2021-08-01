
// Websocket stuff here implements hot reloading (RIP webpack-dev-server..)
const wsUri = (window.location.protocol=='https:'&&'wss://'||'ws://')+window.location.host + '/livereload/';
conn = new WebSocket(wsUri);
conn.onclose = function() {
    setInterval(function() {
        let conn2 = new WebSocket(wsUri);
        conn2.onopen = function() {
            location.reload();
        };
    }, 500);
};