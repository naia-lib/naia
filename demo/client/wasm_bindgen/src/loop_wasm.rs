// Because we hook into web_sys::RtcDataChannel in order to send/receive events
// from the server, we can't just create a simple loop and receive events like
// in loop_native.rs - doing so would block indefinitely and never allow the
// browser to do it's thing! The solution below is to setup a set_timeout() on
// the browser with a callback to the update method below

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        use std::cell::RefCell;
        use std::rc::Rc;

        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        use naia_client_example_app::App;

        pub fn start_loop(app: App) {
            fn set_timeout(f: &Closure<dyn FnMut()>) {
                web_sys::window().unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(f.as_ref().unchecked_ref(), 1)
                    .expect("should register `requestAnimationFrame` OK");
            }

            let mut rc = Rc::new(app);
            let f = Rc::new(RefCell::new(None));
            let g = f.clone();

            let c = move || {
                if let Some(the_app) = Rc::get_mut(&mut rc) {
                    the_app.update();
                };
                set_timeout(f.borrow().as_ref().unwrap());
            };

            *g.borrow_mut() = Some(Closure::wrap(Box::new(c) as Box<dyn FnMut()>));

            set_timeout(g.borrow().as_ref().unwrap());
        }
    } else {}
}
