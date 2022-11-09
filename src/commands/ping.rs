use serenity::{
    builder::CreateApplicationCommand,
    model::Permissions,
};

pub fn register(
    command: &mut CreateApplicationCommand
) -> &mut CreateApplicationCommand {
    command
        .name("delay")
        .description("Delays a session.")
        .default_member_permissions(Permissions::ADMINISTRATOR)
}
