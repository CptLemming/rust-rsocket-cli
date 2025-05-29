# RSocket CLI

Interact with [server reflection](https://grpc.io/docs/guides/reflection/) on an RSocket server.

Based loosely on [grpcurl](https://github.com/fullstorydev/grpcurl).

## List

List available services:

```sh
rsocket-cli ws://localhost:8201/rsocket list
```

Output:

```txt
api.protobuf.routing.RoutingStrips
api.protobuf.eq.EqBand
```

## Describe

### Service

Describe service methods:

```sh
rsocket-cli ws://localhost:8201/rsocket describe api.protobuf.eq.EqEndpoint
```

Output:

```txt
api.protobuf.eq.EqEndpoint is a service:

service EqEndpoint {
  rpc GetBands( api.protobuf.path.PathOrFaderRequest ) returns ( stream api.protobuf.eq.BandsResponse );
  rpc GetGraph( api.protobuf.path.PathOrFaderRequest ) returns ( stream api.protobuf.eq.EqGraphResponse );
}
```

### Service method

Describe service method:

```sh
rsocket-cli ws://localhost:8201/rsocket describe api.protobuf.eq.EqEndpoint/GetBands
```

Output:

```txt
api.protobuf.eq.EqEndpoint.GetBands is a method:

rpc GetBands( api.protobuf.path.PathOrFaderRequest ) returns ( stream api.protobuf.eq.BandsResponse );
```

## Call

Call endpoint:

```sh
rsocket-cli ws://localhost:8201/rsocket api.protobuf.eq.EqBands/GetBands
```

Output:

```json
{"bands":[{},{"index":1},{"index":2},{"index":3},{"index":4},{"index":5}]}
```

Call endpoint with token:

```sh
rsocket-cli ws://localhost:8201/rsocket -t bearerToken api.protobuf.eq.EqBands/GetBands
```

Call endpoint with data:

```sh
rsocket-cli ws://localhost:8201/rsocket api.protobuf.eq.EqBands/UpdateBand -d '{"faderNumber":0,"index":0,"gain":38}
```
