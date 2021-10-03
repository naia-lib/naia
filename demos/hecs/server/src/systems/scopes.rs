use naia_hecs_server::WorldProxy;

use naia_hecs_demo_shared::protocol::Position;

use crate::app::App;

pub fn update_scopes(app: &mut App) {
    // Update scopes of entities
    for (_, user_key, entity_key) in app.server.scope_checks() {
        if let Some(pos_ref) = app
            .server
            .entity(app.world.proxy(), &entity_key)
            .component::<Position>()
        {
            let x = *pos_ref.borrow().x.get();
            if x >= 5 && x <= 100 {
                app.server.user_scope(&user_key).include(&entity_key);
            } else {
                app.server.user_scope(&user_key).exclude(&entity_key);
            }
        }
    }
}
