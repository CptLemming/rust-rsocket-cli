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

Describe service methods:

```sh
rsocket-cli ws://localhost:8201/rsocket describe api.protobuf.eq.EqBands
```

Output:

```txt
api.protobuf.eq.EqBands is a service:

service EqBands {
  rpc UpdateBand( api.protobuf.eq.BandIdent ) returns ( api.protobuf.mutation.MutationResult );
  rpc GetBand( api.protobuf.eq.BandRequest ) returns ( stream api.protobuf.eq.BandResponse );
  rpc GetBands( api.protobuf.eq.BandsRequest ) returns ( stream api.protobuf.eq.BandsResponse );
}
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
