use crate::{world::host::host_world_manager::SubCommandId, EntityCommand};

pub(crate) struct AuthChannelSender {
    next_subcommand_id: SubCommandId,
    outgoing_commands: Vec<EntityCommand>,
}

impl AuthChannelSender {
    pub(crate) fn new() -> Self {
        Self {
            next_subcommand_id: 0,
            outgoing_commands: Vec::new(),
        }
    }

    pub(crate) fn send_command(&mut self, mut command: EntityCommand) {
        command.set_subcommand_id(self.next_subcommand_id);

        // info!("AuthChannelSender sending command. SubId: {:?}, Command: {:?}", self.next_subcommand_id, command);

        self.next_subcommand_id = self.next_subcommand_id.wrapping_add(1);

        self.outgoing_commands.push(command);
    }

    pub(crate) fn drain_messages_into(&mut self, commands: &mut Vec<EntityCommand>) {
        commands.append(&mut self.outgoing_commands);
    }
}
