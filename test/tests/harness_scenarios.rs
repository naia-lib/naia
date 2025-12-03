use naia_test::{
    harness::{Scenario, ClientKey, EntityKey},
    protocol, Auth, Position,
};
use naia_shared::EntityAuthStatus;

/// Test: single client spawn replicates to server
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new(protocol());
    scenario.server_start();
    
    let a = scenario.client_connect(Auth::new("client_a", "password"), "Client A");
    
    // Mutate phase: client spawns entity
    let ent = scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.spawn().with_position(Position::new(1.0, 2.0)).track()
        })
    });
    
    // Expect phase: server has entity
    scenario.expect(|ctx| {
        ctx.server(|sv| {
            sv.has_entity(ent);
        });
    });
}

/// Test: two clients see the same logical entity
#[test]
fn harness_two_clients_entity_mapping() {
    let mut scenario = Scenario::new(protocol());
    scenario.server_start();
    
    let a = scenario.client_connect(Auth::new("client_a", "password"), "Client A");
    let b = scenario.client_connect(Auth::new("client_b", "password"), "Client B");
    
    // Mutate phase: client A spawns entity, server includes B in scope
    let ent = scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.spawn().with_position(Position::new(10.0, 20.0)).track()
        })
    });
    
    scenario.mutate(|ctx| {
        ctx.server(|sv| {
            sv.include_in_scope(b, ent);
        });
    });
    
    // Expect phase: client B sees entity
    scenario.expect(|ctx| {
        ctx.client(b, |c| {
            c.sees(ent);
        });
    });
    
    // Additional expect: both clients report same position after A changes it
    scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).set_position(Position::new(100.0, 200.0));
        });
    });
    
    scenario.expect(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).position_is(100.0, 200.0);
        });
        ctx.client(b, |c| {
            c.entity(ent).position_is(100.0, 200.0);
        });
    });
}

/// Test: delegating authority from A to B (smoke test)
#[test]
fn harness_delegation_flow_smoke() {
    let mut scenario = Scenario::new(protocol());
    scenario.server_start();
    
    let a = scenario.client_connect(Auth::new("client_a", "password"), "Client A");
    let b = scenario.client_connect(Auth::new("client_b", "password"), "Client B");
    
    // Step 1: Client A spawns entity
    let ent = scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.spawn().with_position(Position::new(10.0, 20.0)).track()
        })
    });
    
    // Step 2: Expect server has entity
    scenario.expect(|ctx| {
        ctx.server(|sv| {
            sv.has_entity(ent);
        });
    });
    
    // Step 3: Server includes B in scope
    scenario.mutate(|ctx| {
        ctx.server(|sv| {
            sv.include_in_scope(b, ent);
        });
    });
    
    // Step 4: Client A configures entity as delegated
    scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).delegate();
        });
    });
    
    // Step 5: Expect server sees DelegateEntityEvent, client A sees delegated and Granted
    scenario.expect(|ctx| {
        ctx.server(|sv| {
            sv.event::<naia_server::DelegateEntityEvent>("delegation");
        });
        ctx.client(a, |c| {
            c.entity(ent).replication_is_delegated();
            c.entity(ent).auth_is(EntityAuthStatus::Granted);
        });
    });
    
    // Step 6: Client A releases authority
    scenario.mutate(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).release_auth();
        });
    });
    
    // Step 7: Expect client A sees Available
    scenario.expect(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).auth_is(EntityAuthStatus::Available);
        });
    });
    
    // Step 8: Expect client B sees entity as delegated and Available
    scenario.expect(|ctx| {
        ctx.client(b, |c| {
            c.sees(ent);
            c.entity(ent).replication_is_delegated();
            c.entity(ent).auth_is(EntityAuthStatus::Available);
        });
    });
    
    // Step 9: Client B requests authority
    scenario.mutate(|ctx| {
        ctx.client(b, |c| {
            c.entity(ent).request_auth();
        });
    });
    
    // Step 10: Expect client B sees Granted, client A still sees Available
    scenario.expect(|ctx| {
        ctx.client(b, |c| {
            c.entity(ent).auth_is(EntityAuthStatus::Granted);
        });
        ctx.client(a, |c| {
            c.entity(ent).auth_is(EntityAuthStatus::Available);
        });
    });
    
    // Step 11: Client B sets new position
    scenario.mutate(|ctx| {
        ctx.client(b, |c| {
            c.entity(ent).set_position(Position::new(100.0, 200.0));
        });
    });
    
    // Step 12: Expect client A eventually sees new position
    scenario.expect(|ctx| {
        ctx.client(a, |c| {
            c.entity(ent).position_is(100.0, 200.0);
        });
    });
}

