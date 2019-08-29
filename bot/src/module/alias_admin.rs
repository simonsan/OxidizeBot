use crate::{auth, command, db, module, prelude::*};
use failure::Error;
use parking_lot::RwLock;
use std::sync::Arc;

/// Handler for the !alias command.
pub struct Handler {
    pub aliases: Arc<RwLock<Option<db::Aliases>>>,
}

#[async_trait]
impl command::Handler for Handler {
    async fn handle(&mut self, mut ctx: command::Context<'_>) -> Result<(), Error> {
        let aliases = match self.aliases.read().clone() {
            Some(aliases) => aliases,
            None => return Ok(()),
        };

        let next = command_base!(ctx, aliases, "alias", AliasEdit);

        match next.as_ref().map(String::as_str) {
            Some("edit") => {
                ctx.check_scope(auth::Scope::AliasEdit)?;

                let name = ctx_try!(ctx.next_str("<name>"));
                let template = ctx_try!(ctx.rest_parse("<name> <template>"));
                aliases.edit(ctx.channel(), &name, template)?;

                ctx.respond("Edited alias");
            }
            None | Some(..) => {
                ctx.respond("Expected: show, list, edit, delete, enable, disable, or group.");
            }
        }

        Ok(())
    }
}

pub struct Module;

impl super::Module for Module {
    fn ty(&self) -> &'static str {
        "alias"
    }

    fn hook(
        &self,
        module::HookContext {
            injector, handlers, ..
        }: module::HookContext<'_, '_>,
    ) -> Result<(), Error> {
        handlers.insert(
            "alias",
            Handler {
                aliases: injector.var()?,
            },
        );
        Ok(())
    }
}
