The Naia mdBook looks amazing! But in proof-reading it all, I find several issues which need to be resolved. Going to track them here:

1. We need to be consistent across the book that the current WebRTC transport mechanism works for both native AND Wasm clients, simultaneously from the same server. UDP transport is not required to support native clients.
2. We need to be consistent across the book that the server is NOT always the authoritative source of truth over the ECS world. Naia supports client-authoritative entities, which can be spawned/despawned/updated on the client and synced to the server. Users must explicitly enable client-authoritative functionality in the protocol, it's opt-in.
3. The "delegated authority" system is separate but related to purely client-authoritative entities - it provides a pattern to allow either server or client to toggle who holds authority over a given entity at any time.

## Introduction Page - specific

4. Currently says "Browser clients - the only Rust library with a production WebRTC transport for wasm32-unknown-unknown, sharing all protocol and game logic with the native client" this is inaccurate. `lightyear` also supports Wasm clients (via WebTransport transport). `bevy_replicon` supports Wasm clients since it is transport-agnostic, and use a crate like `aeronet_webtransport` to support Wasm clients.
5. I don't want to advertise the internal runtime with this line: "No tokio dependency — naia uses smol / async-std internally, fitting cleanly into stacks that already use those runtimes".
6. The "Crate map" section lists `naia-macroquad-client`, which is non-existent (macroquad clients can just use `naia` directly without an adapter). It's also missing `naia-bevy-shared`. There are a lot of other internal crates we COULD list here ... but I'm unsure if that's really helpful for users.
7. I think the "Why naia stands out" section could list out more of our unique selling points.
8. In the text "naia is ECS-agnostic. the core crate works with any entity type that is Copy + Eq + Hash + Send + Sync.", I don't really want to bog the reader down with implementation specifics here.
9. Quick concepts section should include "Message" type, and I think there are other terminologies that should be mentioned as well. Also, the "TickBuffered" explanation in "Tick" seems out of place here.

## "Why naia?" page - specific

10. Again here, naia does not have a monopoly in the Rust ecosystem on browser-capable clients, let's not advertise that.
11. We shouldn't advertise the async runtime used internally (smol / async-std) either - it's an implementation detail.
12. "The short answer" section should do better at selling naia, the list of features does not put the best parts front and center. Just look at the 5 bullet points there and see how it is a bit underwhelming.
13. The "relationship to Tribes 2" section here seems out of place for this page.

## Installation page - specific

14. The user's app's "shared" crate (if this is a bevy-based project) should use `naia-bevy-shared` (not `naia-shared`)
15. The user's app's "client" and "server crates (if this is a bevy-based project) should only need to use `naia-bevy-client` and `naia-bevy-server` respectively, not `naia-shared`.. Shared primitives are re-exported by those crates. Same concept applies to if it's a non-bevy project.
16. The section title "Core (no ECS)" does not make sense there.
17. Again, there's no such thing as `naia-macroquad-client`
18. Double check the "Browser (WASM) target" section to make sure it's consistent with the above points.

## Bevy Quick Start page - specific

19. Again, the example `my-game-shared` crate here should be using `naia-bevy-shared` (not `naia-shared`)
20. Otherwise this looks good!

## "Your First X" & "The Shared Protocol" - specific

21. Yeah these pages look good! Just double check we don't have any redundant information here (from the Bevy Quick Start page) and that everything is correct and consistent!

## Entity Replication page - specific

22. The "replication state machine" section has a syntax error in the mermaid diagram. How can we ensure CI catches this kind of thing so this doesn't happen again?
23. This goes for the "Replicated Resources" globally too, but here in the "Replicated Resources" section, it says that the "Replicated Resource" feature is a server-side singleton ... Not true actually, I believe we also have the concept of client-authoritative replicated resources. Do your research.

## "Messages & Channels" & "Rooms & Scoping" & "Tick Synchronization" & "Connection Lifecycle" - specific

24. These looks great! Just double check everything is correct and consistent with the rest of the documentation.

## "Server Authority" page - specific

25. Again, we may just want to mention that there are client-authoritative entities/resources too, even without the Delegation/Authority model. Some of the statements in this page seem to indicate the opposite. Do your research.

## "Client-owned Entities" page - specific

26. This statement: "Publicity::Private keeps the entity purely client-local — it is never sent to the server." is false. `Publicity::Private` still allows the entity to be replicated to the server, it just keeps it from being sent to other clients. `Publicity::Public` allows the client-owned entity to ALSO be replicated to other clients (if it is in scope for those clients).

## "Authority Delegation" page - specific

27. The "By default the server owns all component state." is wrong. Client-owned entities/resources can be migrated to a Delegated state by the client, which migrates the entity to be server-owned and enables delegation for that entity.
28. Make sure, here and in other pages, that we note that in Bevy projects, any entity that is to be replicated must call the "enable_replication()" function on that entity. Otherwise it will stay completely local to the host world it's on.
29. The "Common patterns:" part in the "Per-user authority" section has not been proven out, maybe shouldn't be mentioned.
30. Again, make sure the "Publicity" information here is accurate - it looks like there are some false claims here.

## "Entity Publishing" & "Client-Side Prediction & Rollback" - specific

31. This looks fine .. could be streamlined a bit. Also just check for accuracy.

## "Lag Compensation with Historian" - specific

32. This looks good! The "In a server-authoritative game each client renders the world a little in the past" part may be a bit confusing though, especially for users who have implemented Prediction & Rollback in their game. Typically, in a client-predicted game, each client has a "Confirmed" world, and a "Predicted" world, each on their own timelines. The client's "Predicted" world runs ahead of the Server's timeline. The client's "Confirmed" world runs behind the Server's timeline (as described already, typically at `RTT/2 + interpolation_buffer` behind the server). Let's make this a bit more easier to understand here.
33. Let's double check the claim: "This is a naia-exclusive feature — no other Rust game networking library ships a built-in historian primitive"

## "Priority-Weighted Bandwidth" & "Delta Compression" & "zstd Compression & Dictionary training" pages - specific

34. Looks good here to me!

## "Request / Response" page - specific

35. I noticed that the snippets in here use the core `naia_*` crates, not the `naia_bevy_*` crates. The mdBook is intended to be targeted at Bevy users, with non-Bevy specifics mentioned in the "Without Bevy" page grouping. Let's update the snippets to use `naia_bevy_*` crates, here and throughout the book (except for the "Without Bevy" pages).
36. Just curious ... I thought we had derive macros for `Request` and `Response`, that doesn't require manual implementation. Am I remembering this wrong?
37. Looks good otherwise!

## "Transport" > "Overview" page - specific

38. It says in the table here that `transport_udp` is intended to be used by native targets, and that `transport_webrtc` is for web targets only. That's not the case! `transport_webrtc` is available for native targets as well, and is probably preferable for the extra functionality (like DTLS encryption, handshaking, ect) that it provides.
39. I don't want anything in this book to reference a `transport_quic` feature.. That is hypothetical and may never be implemented.
40. I'm pretty sure there is no `naia-socket-native`, `naia-socket-webrtc` or `naia-socket-local` crates! Where did you get this?? I believe different transports are just selected via feature flags!

## "Transport" > "Native UDP" | "WebRTC (Browser Clients)" | "Local (In-Process)" pages - specific

41. Pretty sure this all looks good, but just make sure that everything here is up-to-date and accurate with the actual codebase.

## "Transport" > "Writing a Custom Transport" page - specific

42. I think this is right, but it looks pretty minimal, make sure you've covered everything here!

## "Without Bevy" page grouping - specific

43. This all looks right, but I'd prefer the snippets do NOT indicate that `async` is required for core naia functionality.
44. This page grouping looks quite minimal, please take a step back, look at this section from the point of view of a non-Bevy user that wants to use naia still, or a Macroquad user, ect.. And make these pages more comprehensive and accurate to help those readers.

## "Performance & Diagnostics" page grouping - specific

45. I think this all looks great! Just make sure this is high-quality and accurate and consistent information here!

## "Live Demo" - specific

46. For now, let's just not include this in the book. Let's wait till the live demo is functional before putting this page in here!

## "Feature Matrix" page - specific

47. This page looks pretty good! Just make sure it's comprehensive, and organized and readable. Some of it is a bit dense and ramshackle, you know? Everything here should be features at the same granularity detail - lets not include EVERY little implementation detail, you know? Is this all properly organized? Probably for such a long list there should be groupings of features.
48. `transport_quic` is NOT planned, and "Per-component replication toggle" will NEVER be implemented, by design. Let's maybe not even worry about mentioning future mobile client work here either.

## "Security & Trust Model" page - specific

49. I don't think that `transport_udp` should be the primary transport to advertise to users. This is a principle that should be applied throughout the book. Using `transport_udp` indicates you are willing to do the work to secure your connections, and that you understand the risks involved. I believe the `transport_webrtc` transport is currently the more production-ready and secure choice, and it works on both native and web targets right now, so please rewrite this section to reflect that.
50. Along with the above, I don't think the "Securing native UDP deployments today" section is necessary here.

## "Comparing naia to Alternatives" page - specific

51. Let's make sure in our feature matrix here we put naia's best features forward toward the top, and make the feature list genuinely comprehensive, without including finer grained functionality that users are unlikely to appreciate, or implementation details. Make sure the feature availability in `naia` is accurate!
52. In the feature matrix, `lightyear` and `bevy_replicon` definitely deserve to be here on this list, but I'm not so sure `renet` and `ggrs` should be, especially because they may live at a slightly different layer to `naia` which makes it a bit "apples vs oranges". I definitely want to include `matchbox` here as a comparison target here though!
53. Again, `lightyear` and `bevy_replicon` DO support browser clients, let's be accurate here in our comparisons.
54. For the `naia vs X` sections, yeah let's just make sure these are accurate, and put `naia`'s best features at the top of the lists.

## "Bevy Adapter Deep Dive" page - specific

55. These mostly look good, but just make sure it's relevant and readable and comprehensive and detailed. What user is reading this section? What are they looking for? Is this all accurate?

## "Glossary" page - specific

56. Please check through all these and make sure all entries are truly valuable and accurate. I'm not sure these should be in an alphabetical order, they maybe should be put into several groupings.

## "Migration Guide" page - specific

57. Please get rid of this page for now.

## "API Docs (docs.rs)" page - specific

58. There is no such thing as a `naia-macroquad-client`, in this list! Remove it!
59. Make sure this list includes ALL published crates in the `naia` repository!

## "FAQ" page - specific

60. This is all over the place. Let's make sure it's organized and easy to find answers to common questions. Make sure these are ACTUALLY questions people might ask.

## "Changelog" page - specific

61. Maybe this should include our actual changelog instead of linking to the file in GitHub?
