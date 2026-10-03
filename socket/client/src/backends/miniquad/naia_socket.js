const naia_socket = {
    encoder: new TextEncoder(),
    decoder: new TextDecoder("utf-8"),
    js_objects: {},
    unique_js_id: 0,
    // One connection record per socket id, filed by `connect` under the id
    // the Rust half allocated. A second socket adds a record; it never
    // replaces the first socket's channel (naia-lib/naia#193). The transient
    // `js_objects` above stay shared: they are argument marshaling, not
    // connection state.
    connections: {},

    plugin: function (importObject) {
        importObject.env.naia_is_connected = function (socket_id) { return naia_socket.is_connected(socket_id); };
        importObject.env.naia_connect = function (socket_id, server_socket_address, rtc_path, auth_str, ice_servers, protocol_id) { return naia_socket.connect(socket_id, server_socket_address, rtc_path, auth_str, ice_servers, protocol_id); };
        importObject.env.naia_disconnect = function (socket_id) { naia_socket.disconnect(socket_id); };
        importObject.env.naia_send = function (socket_id, message) { return naia_socket.send(socket_id, message); };
        importObject.env.naia_create_string = function (buf, max_len) { return naia_socket.js_create_string(buf, max_len); };
        importObject.env.naia_unwrap_to_str = function (js_object, buf, max_len) { naia_socket.js_unwrap_to_str(js_object, buf, max_len); };
        importObject.env.naia_string_length = function (js_object) { return naia_socket.js_string_length(js_object); };
        importObject.env.naia_create_u8_array = function (buf, max_len) { return naia_socket.js_create_u8_array(buf, max_len); };
        importObject.env.naia_unwrap_to_u8_array = function (js_object, buf, max_len) { naia_socket.js_unwrap_to_u8_array(js_object, buf, max_len); };
        importObject.env.naia_u8_array_length = function (js_object) { return naia_socket.js_u8_array_length(js_object); };
        importObject.env.naia_free_object = function (js_object) { naia_socket.js_free_object(js_object); };
        importObject.env.naia_random = function () { return Math.random(); };
        importObject.env.naia_now = function () { return Date.now(); };
    },

    is_connected: function(socket_id) {
        let connection = this.connections[socket_id];
        // The channel object exists from connect(), long before the link is
        // usable: only an open data channel reads as connected (Roger 42320).
        // Presence alone reported true through the whole ICE/DTLS setup
        // window, promoting a connecting socket to connected.
        if (connection && connection.channel && connection.channel.readyState === "open") {
            return true;
        } else {
            return false;
        }
    },

    connect: function (socket_id, server_socket_address, rtc_path, auth_str, ice_servers, protocol_id) {
        let server_socket_address_string = naia_socket.get_js_object(server_socket_address);
        let rtc_path_string = naia_socket.get_js_object(rtc_path);
        let auth_string = naia_socket.get_js_object(auth_str);
        let ice_servers_string = naia_socket.get_js_object(ice_servers);
        let protocol_id_string = naia_socket.get_js_object(protocol_id);
        let SESSION_ADDRESS = server_socket_address_string + rtc_path_string;

        let peer = new RTCPeerConnection({
            iceServers: [{
                urls: JSON.parse(ice_servers_string)
            }]
        });

        let connection = { channel: null, peer: peer, first_send_logged: false };
        naia_socket.connections[socket_id] = connection;

        connection.channel = peer.createDataChannel("data", {
            ordered: false,
            maxRetransmits: 0
        });

        connection.channel.binaryType = "arraybuffer";

        connection.channel.onopen = function() {
            console.log("naia: datachannel onopen", Date.now());
            connection.channel.onmessage = function(evt) {
                let array = new Uint8Array(evt.data);
                wasm_exports.receive(socket_id, naia_socket.js_object(array));
            };
        };

        connection.channel.onerror = function(evt) {
            naia_socket.error(socket_id, "data channel error", evt.message);
        };

        // Candidate counting and the bounded gather gate live in the offer
        // block below: registering the counter there (synchronously inside
        // the setLocalDescription continuation, before gathering can emit)
        // misses no candidates and keeps one decision point.

        peer.createOffer().then(function(offer) {
            return peer.setLocalDescription(offer);
        }).then(function() {
            // The offer is worthless without candidates, and naia's
            // signaling is a single POST offer → answer with no trickle
            // channel to recover later candidates — so a candidate-less
            // early post is never allowed. But gating the send on
            // gathering-complete with no bound tied to the connection
            // deadline stalls connect for ~40 s when a single STUN path is
            // slow (Roger 41874: icecandidateerror 701 held `complete` past
            // the 30 s deadline despite usable srflx/host candidates from
            // ~1 s in). Post on `complete`, or once 10000 ms have passed
            // with at least one candidate in hand — whichever comes first
            // (parity with the wasm_bindgen backend and
            // ICE_GATHER_EARLY_POST_MS in naia-socket-shared; keep the bound
            // in sync). The SDP is read at send time: gathering rewrites
            // the local description in place, so a snapshot taken now would
            // post the pre-gathering text.
            let settled = false;
            let candidateCount = 0;
            const gatherStartMs = Date.now();
            // Sized generously: typical networks complete in a few seconds,
            // but constrained ones were measured at ~40s of candidate-probing
            // tail before "complete" fires.
            let timer = setTimeout(function() {
                if (settled) return;
                settled = true;
                naia_socket.error(socket_id, "ice gathering did not complete within 60000ms: session offer never posted", null);
            }, 60000);
            function maybe_post_offer() {
                if (settled) return;
                const complete = peer.iceGatheringState === "complete";
                const elapsedMs = Date.now() - gatherStartMs;
                if (!complete && !(elapsedMs >= 10000 && candidateCount >= 1)) return;
                settled = true;
                clearTimeout(timer);
                clearTimeout(earlyTimer);
                post_offer();
            }
            peer.onicecandidate = function(evt) {
                if (evt.candidate) {
                    candidateCount += 1;
                    console.log("received ice candidate", evt.candidate);
                } else {
                    console.log("all local candidates received");
                }
                maybe_post_offer();
            };
            function post_offer() {
            let request = new XMLHttpRequest();
            console.log("naia: session POST send", Date.now(), SESSION_ADDRESS);
            request.open("POST", SESSION_ADDRESS);
            if (auth_string.length > 0) {
                request.setRequestHeader("Authorization", auth_string);
            }
            // Set last and unconditionally: naia owns this header. The server
            // refuses the request outright if it is missing or does not match,
            // so there is no "connect without it" path here either.
            request.setRequestHeader("x-naia-protocol-id", protocol_id_string);
            request.onload = function() {
                console.log("naia: session POST status", request.status);
                if (request.status === 200) {
                    let response = JSON.parse(request.responseText);
                    // Shape only, never the value: the id is an auth secret.
                    let id_length = (typeof response.id === "string") ? response.id.length : -1;
                    console.log("naia: session id", typeof response.id, id_length);

                    wasm_exports.receive_id(socket_id, naia_socket.js_object(response.id));

                    peer.setRemoteDescription(new RTCSessionDescription(response.sdp.answer)).then(function() {
                        let response_candidate = response.sdp.candidate;
                        wasm_exports.receive_candidate(socket_id, naia_socket.js_object(JSON.stringify(response_candidate.candidate)));
                        let candidate = new RTCIceCandidate(response_candidate);
                        peer.addIceCandidate(candidate).then(function() {
                            console.log("add ice candidate success");
                        }).catch(function(err) {
                            naia_socket.error(socket_id, "error during 'addIceCandidate'", err);
                        });
                    }).catch(function(err) {
                        naia_socket.error(socket_id, "error during 'setRemoteDescription'", err);
                    });
                } else {
                    // A completed POST with a non-200 status is a signaling
                    // answer, not a transport failure: 401/409 carry the
                    // rejection the identity path must surface, so they go to
                    // the dedicated auth-error callback with status and body,
                    // never through the generic packet error queue, and never
                    // wait on a data channel a rejected handshake will not
                    // create. Network-level failures (onerror, no status) stay
                    // generic below.
                    wasm_exports.receive_auth_error(
                        socket_id,
                        naia_socket.js_object(String(request.status)),
                        naia_socket.js_object(request.responseText || "")
                    );
                }
            };
            request.onerror = function(err) {
                let error_str = "error sending POST request to " + SESSION_ADDRESS;
                naia_socket.error(socket_id, error_str, err);
            };
            request.send(peer.localDescription.sdp);
            }
            peer.onicegatheringstatechange = maybe_post_offer;
            // Bounded wait: fires once the early-post bound has passed.
            // Posts iff candidates arrived by then; with zero candidates
            // this is a no-op and the 60 s backstop above still reports
            // loudly. A candidate arriving after this timer fired still
            // posts via onicecandidate, since the wait has by then passed
            // with candidates in hand.
            let earlyTimer = setTimeout(function() {
                maybe_post_offer();
            }, 10000);
            maybe_post_offer();
        }).catch(function(err) {
            naia_socket.error(socket_id, "error during 'createOffer'", err);
        });
    },

    disconnect: function(socket_id) {
        delete this.connections[socket_id];
    },

    error: function (socket_id, desc, err) {
        err['naia_desc'] = desc;
        wasm_exports.error(socket_id, this.js_object(JSON.stringify(err)));
    },

    send: function (socket_id, message) {
        let message_string = naia_socket.get_js_object(message);
        return this.send_u8_array(socket_id, message_string);
    },

    js_create_string: function (buf, max_len) {
        let string = UTF8ToString(buf, max_len);
        return this.js_object(string);
    },

    js_unwrap_to_str: function (js_object, buf, max_len) {
        let str = this.js_objects[js_object];
        let utf8array = this.toUTF8Array(str);
        let length = utf8array.length;
        let dest = new Uint8Array(wasm_memory.buffer, buf, max_len);
        for (let i = 0; i < length; i++) {
            dest[i] = utf8array[i];
        }
    },

    js_string_length: function (js_object) {
        let str = this.js_objects[js_object];
        return this.toUTF8Array(str).length;
    },

    send_u8_array: function (socket_id, str) {
        let connection = this.connections[socket_id];
        if (connection && connection.channel) {
            if (!connection.first_send_logged) {
                connection.first_send_logged = true;
                console.log("naia: first send readyState", connection.channel.readyState);
            }
            try {
                connection.channel.send(str);
                return true;
            }
            catch(err) {
                return false;
            }
        } else {
            return false;
        }
    },

    js_create_u8_array: function (buf, max_len) {
        let u8Array = new Uint8Array(wasm_memory.buffer, buf, max_len);
        return this.js_object(u8Array);
    },

    js_unwrap_to_u8_array: function (js_object, buf, max_len) {
        let str = this.js_objects[js_object];
        let length = str.length;
        let dest = new Uint8Array(wasm_memory.buffer, buf, max_len);
        for (let i = 0; i < length; i++) {
            dest[i] = str[i];
        }
    },

    js_u8_array_length: function (js_object) {
        let str = this.js_objects[js_object];
        return str.length;
    },

    js_free_object: function (js_object) {
        delete this.js_objects[js_object];
    },

    toUTF8Array: function (str) {
        let utf8 = [];
        for (let i = 0; i < str.length; i++) {
            let charcode = str.charCodeAt(i);
            if (charcode < 0x80) utf8.push(charcode);
            else if (charcode < 0x800) {
                utf8.push(0xc0 | (charcode >> 6),
                    0x80 | (charcode & 0x3f));
            }
            else if (charcode < 0xd800 || charcode >= 0xe000) {
                utf8.push(0xe0 | (charcode >> 12),
                    0x80 | ((charcode >> 6) & 0x3f),
                    0x80 | (charcode & 0x3f));
            }
            // surrogate pair
            else {
                i++;
                // UTF-16 encodes 0x10000-0x10FFFF by
                // subtracting 0x10000 and splitting the
                // 20 bits of 0x0-0xFFFFF into two halves
                charcode = 0x10000 + (((charcode & 0x3ff) << 10)
                    | (str.charCodeAt(i) & 0x3ff))
                utf8.push(0xf0 | (charcode >> 18),
                    0x80 | ((charcode >> 12) & 0x3f),
                    0x80 | ((charcode >> 6) & 0x3f),
                    0x80 | (charcode & 0x3f));
            }
        }
        return utf8;
    },

    js_object: function (obj) {
        let id = this.unique_js_id;
        this.js_objects[id] = obj;
        this.unique_js_id += 1;
        return id;
    },

    get_js_object: function (id) {
        return this.js_objects[id];
    }
};

miniquad_add_plugin({ register_plugin: naia_socket.plugin, version: "0.14.0", name: "naia_socket" });
