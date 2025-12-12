use std::net::SocketAddr;

use naia_server::{UserKey};

use crate::{Auth, ClientKey, harness::{server_expect_ctx::ServerExpectCtx, client_expect_ctx::ClientExpectCtx}};

pub trait TranslatableEvent<Ctx> {
    type Input;
    type Output;

    fn translate_item(ctx: &Ctx, input: Self::Input) -> Option<Self::Output>;
}

//// Server Event Implementations ////

impl<'a> TranslatableEvent<ServerExpectCtx<'a>> for naia_server::AuthEvent<Auth>
{
    type Input = (UserKey, Auth);
    type Output = (ClientKey, Auth);

    fn translate_item(
        ctx: &ServerExpectCtx<'a>,
        input: Self::Input,
    ) -> Option<Self::Output> {
        let (user_key, auth) = input;
        ctx.scenario()
            .user_to_client_key(&user_key)
            .map(|client_key| (client_key, auth))
    }
}

impl<'a> TranslatableEvent<ServerExpectCtx<'a>> for naia_server::ConnectEvent
{
    type Input = UserKey;
    type Output = ClientKey;

    fn translate_item(
        ctx: &ServerExpectCtx<'a>,
        input: Self::Input,
    ) -> Option<Self::Output> {
        let user_key = input;
        ctx.scenario().user_to_client_key(&user_key)
    }
}

//// Client Event Implementations ////

impl<'a> TranslatableEvent<ClientExpectCtx<'a>> for naia_client::ConnectEvent
{
    type Input = SocketAddr;
    type Output = ();

    fn translate_item(
        _ctx: &ClientExpectCtx<'a>,
        _input: Self::Input,
    ) -> Option<Self::Output> {
        Some(())
    }
}