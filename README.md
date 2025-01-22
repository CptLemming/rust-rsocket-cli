# RSocket CLI

Interact with [server reflection](https://grpc.io/docs/guides/reflection/) on an RSocket server.

Based loosely on [grpcurl](https://github.com/fullstorydev/grpcurl).

```sh
rsocket-cli ws://localhost:8201/rsocket list
# Service : "api.protobuf.routing.RoutingStrips"
# Service : "api.protobuf.eq.EqBands"

rsocket-cli ws://localhost:8201/rsocket describe api.protobuf.eq.EqBands
# File : "..."
# File : "..."
# File : "..."
rsocket-cli ws://localhost:8201/rsocket api.protobuf.routing.RoutingStrips/GetRoutingStrips
# Entry : {...}
rsocket-cli ws://localhost:8201/rsocket api.protobuf.routing.RoutingStrips/RouteMain -d '{"faderNumber":0,"index": 0}'
# Entry : {...}
rsocket-cli ws://localhost:8080/rsocket renderer.RendererService/OnEvent
# Entry : {...}
```
