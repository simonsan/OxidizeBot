use crate::{auth, command, module, prelude::*};
use parking_lot::RwLock;
use std::sync::Arc;

static MAGIC_8BALL_ANSWER: &[&'static str] = &[
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes - definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful.",
];

/// Handler for the !8ball command.
pub struct EightBall {
    enabled: Arc<RwLock<bool>>,
}

#[async_trait]
impl command::Handler for EightBall {
    fn scope(&self) -> Option<auth::Scope> {
        Some(auth::Scope::EightBall)
    }

    async fn handle(&mut self, ctx: command::Context<'_>) -> Result<(), failure::Error> {
        use rand::Rng as _;

        if !*self.enabled.read() {
            return Ok(());
        }

        let rest = ctx.rest();

        if rest.trim().is_empty() {
            ctx.respond("Ask a question.");
            return Ok(());
        }

        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0, MAGIC_8BALL_ANSWER.len());

        if let Some(answer) = MAGIC_8BALL_ANSWER.get(index) {
            ctx.respond(answer);
        }

        Ok(())
    }
}

pub struct Module;

#[async_trait]
impl super::Module for Module {
    fn ty(&self) -> &'static str {
        "8ball"
    }

    /// Set up command handlers for this module.
    async fn hook(
        &self,
        module::HookContext {
            handlers, settings, ..
        }: module::HookContext<'_, '_>,
    ) -> Result<(), failure::Error> {
        handlers.insert(
            "8ball",
            EightBall {
                enabled: settings.var("8ball/enabled", true)?,
            },
        );

        Ok(())
    }
}
