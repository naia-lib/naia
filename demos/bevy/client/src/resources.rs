use naia_bevy_demo_shared::protocol::KeyCommand;

pub struct Global {
    pub queued_command: Option<KeyCommand>,
}
