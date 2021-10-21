
## [0.7.0-alpha.1]
- Lots and lots of refactors, I should've been taking notes since the beginning :)
- Refactored to use ECS concepts
- Refactored to store networked components in an external ECS world, rather than internal to the Client/Server
- Added new 'adapters' for Bevy and Hecs, which are helper libraries to integrate with Naia
- Added various new demos

## [0.4.0]
- Moved completely off of tokio & in the demos too (prefer smol & runtime-agnostic where necessary)
- Update to use naia-socket version 0.4.0

--- sorry for skipped changelog here ---

## [0.1.1]
- Bringing in a fix from naia-socket, where [Slyklaw](https://github.com/Slyklaw) fixed a Windows incompatibility regarding the lookup of the host's ip address

## [0.1.0]
- Initial release