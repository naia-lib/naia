const naia_socket = {
    channel: null,
    encoder: new TextEncoder(),
    decoder: new TextDecoder("utf-8"),
    js_objects: {},
    unique_js_id: 0,

    plugin: function (importObject) {
        importObject.env.naia_is_connected = function () { return naia_socket.is_connected(); };
        importObject.env.naia_connect = function (address, rtc_path, auth_str) { naia_socket.connect(address, rtc_path, auth_str); };
        importObject.env.naia_disconnect = function () { naia_socket.disconnect(); };
        importObject.env.naia_send = function (message) { return naia_socket.send(message); };
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

    is_connected: function() {
        if (this.channel) {
            return true;
        } else {
            return false;
        }
    },

    connect: function (server_socket_address, rtc_path, auth_str) {
        let server_socket_address_string = naia_socket.get_js_object(server_socket_address);
        let rtc_path_string = naia_socket.get_js_object(rtc_path);
        let SESSION_ADDRESS = server_socket_address_string + rtc_path_string;

        let peer = new RTCPeerConnection({
            iceServers: [{
                urls: ["stun:stun.l.google.com:19302"]
            }]
        });

        this.channel = peer.createDataChannel("data", {
            ordered: false,
            maxRetransmits: 0
        });

        this.channel.binaryType = "arraybuffer";

        this.channel.onopen = function() {
            naia_socket.channel.onmessage = function(evt) {
                let array = new Uint8Array(evt.data);
                wasm_exports.receive(naia_socket.js_object(array));
            };
        };

        this.channel.onerror = function(evt) {
            naia_socket.error("data channel error", evt.message);
        };

        peer.onicecandidate = function(evt) {
            if (evt.candidate) {
                console.log("received ice candidate", evt.candidate);
            } else {
                console.log("all local candidates received");
            }
        };

        peer.createOffer().then(function(offer) {
            return peer.setLocalDescription(offer);
        }).then(function() {
            let request = new XMLHttpRequest();
            request.open("POST", SESSION_ADDRESS);
            if (auth_str.length > 0) {
                request.setRequestHeader("Authorization", auth_str);
            }
            request.onload = function() {
                if (request.status === 200) {
                    let response = JSON.parse(request.responseText);

                    wasm_exports.receive_id(naia_socket.js_object(response.id));

                    peer.setRemoteDescription(new RTCSessionDescription(response.sdp.answer)).then(function() {
                        let response_candidate = response.sdp.candidate;
                        wasm_exports.receive_candidate(naia_socket.js_object(JSON.stringify(response_candidate.candidate)));
                        let candidate = new RTCIceCandidate(response_candidate);
                        peer.addIceCandidate(candidate).then(function() {
                            console.log("add ice candidate success");
                        }).catch(function(err) {
                            naia_socket.error("error during 'addIceCandidate'", err);
                        });
                    }).catch(function(err) {
                        naia_socket.error("error during 'setRemoteDescription'", err);
                    });
                } else {
                    let error_str = "error sending POST request to " + SESSION_ADDRESS;
                    naia_socket.error(error_str, { response_status: request.status });
                }
            };
            request.onerror = function(err) {
                let error_str = "error sending POST request to " + SESSION_ADDRESS;
                naia_socket.error(error_str, err);
            };
            request.send(peer.localDescription.sdp);
        }).catch(function(err) {
            naia_socket.error("error during 'createOffer'", err);
        });
    },

    disconnect: function() {
        if (this.channel) {
            this.channel = null;
        }
    },

    error: function (desc, err) {
        err['naia_desc'] = desc;
        wasm_exports.error(this.js_object(JSON.stringify(err)));
    },

    send: function (message) {
        let message_string = naia_socket.get_js_object(message);
        return this.send_u8_array(message_string);
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

    send_u8_array: function (str) {
        if (this.channel) {
            try {
                this.channel.send(str);
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