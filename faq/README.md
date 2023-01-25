# Frequently Asked Questions

## Table of contents

<!-- TOC -->
  * [General question?](#general-question)
    * [What is the difference between the different crates?](#what-is-the-difference-between-the-different-crates)
    * [Is naia compatible with other transport layers?](#is-naia-compatible-with-other-transport-layers)
    * [What game networking concept does naia provide?](#what-game-networking-concept-does-naia-provide)
  * [ECS replication system questions](#ecs-replication-system-questions)
    * [How can I have different replication frequencies per entity or component?](#how-can-i-have-different-replication-frequencies-per-entity-or-component)
    * [What is the tick for?](#what-is-the-tick-for)
    * [What is the difference between `duplicate_entity` and `mirror_entity`](#what-is-the-difference-between-duplicateentity-and-mirrorentity)
    * [How can I know the type of entity that I am replicating?](#how-can-i-know-the-type-of-entity-that-i-am-replicating)
  * [Message/Event passing](#messageevent-passing)
    * [Can any message be passed through a channel? Can I send any struct as an Event?](#can-any-message-be-passed-through-a-channel-can-i-send-any-struct-as-an-event)
    * [What is `Property<>`?](#what-is-property-)
    * [What does `Protocolize` provide?](#what-does-protocolize-provide)
    * [Can I have multiple `Protocolize` enums?](#can-i-have-multiple-protocolize-enums)
<!-- TOC -->

## General question?

### What is the difference between the different crates?

- `naia-socket` is the transport layer (at the same level as UDP/TCP/QUIC) and provides an implementation of the WebRTC protocol to send packets between server and client.
- `naia-shared` contains wrappers around the transport layer. In particular it provides: 
  - a connection abstraction
  - a channel abstraction (how to receive groups of packets)
  - utilities to efficiently serialize your data (with delta-compression and bitpacking)
- `naia-client` and `naia-server` are an opiniated game networking implementation based on the Tribes 2 networking model. They contain 2 main parts:
  - an ECS replication system, where entities/components on the server get automatically replicated to the clients
  - a message-passing system. The channels/connections from the `naia-shared` can also be used to send any kind of message
  - There is a plan to separate those two parts
- `adapters` contains actual ECS implementations. Naia doesn't work as is and needs to combine with an external ECS tool.

### Is naia compatible with other transport layers?

No. Naia is currently only compatible with `naia-socket`, but there are plans to make it abstract over any transport layer.

### What game networking concept does naia provide?

Naia provides a message-passing abstraction with different types of channels (unreliable, reliable, sequenced).
Naia provides efficient serialization with bitpacking and deltacompression.
Naia provides ECS replication. (TODO: elaborate)
Naia provides state-synchronization. (TODO: elaborate)

Naia does NOT provide:
- client-prediction. This is usually fairly game specific and should be implemented by the user.
- lag compensation
- snapshot interpolation


## ECS replication system questions

### How can I have different replication frequencies per entity or component?

This is not possible right now, it is a future intended feature.

### What is the tick for?

On the server, the tick is simply the frequency at which the packets will be sent. (every tick, we call `send_all_updates`).
On the client, the tick information is used to sync up the `update_components` event to happen on the same tick as the server.

### What is the difference between `duplicate_entity` and `mirror_entity`

`duplicate_entity`: create a new entity on the client and copy all the components from the server entity.
`mirror_entity`: take an existing client entity, and copy all the component values from the server entity. (for their common components)

### How can I know the type of entity that I am replicating?

A common use-case is this one. We are replicating multiple different types of entities from the server to the client (food, player, item).
When we replicate the entity on the client, we want to add other components depending on the type of entity being replicated.
For example, for food entities we want to add a food texture, but for player entities we want to add a player texture.

There are 2 options:
- either you can add a 'Marker' component on the entity. When that component gets replicated, then you can insert other components
- either you can choose to not use the ECS replication system and just send different messages such as `FoodMessage` and `PlayerMessage`


## Message/Event passing

### Can any message be passed through a channel? Can I send any struct as an Event?

The structs that can be passed as messages/events need to implement `Replicate`. As such, the fields of the struct need to use the `Property<>` wrapper.

### What is `Property<>`?

`Property<>` is a wrapper that enables change-detection. It is useful only for ECS replication, where `naia` uses it to perform delta-compression: if a component did not change, only 0 or 1 bit of data can be sent through the network.
Messages/Event struct fields still need to have `Property<>`, even though it has no effect in that case.
There are plans to change this and be able to pass any struct as a message.

### What does `Protocolize` provide?

`Protocolize` defines how your data will get serialized and passed through channels.
It provides 2 main optimizations:
- delta-compression for ECS replication via the `Replicate` and `Property` traits
- bit-packing for smaller serialization

### Can I have multiple `Protocolize` enums?

This is not possible right now, but it is planned for future releases.
