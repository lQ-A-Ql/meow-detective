# Go bbolt fixture

`go-bbolt.db` was generated with Go bbolt v1.4.3 and checked with its `Tx.Check`
before closing it. It is a static interoperability input; building and testing
this repository require Rust and do not require Go or any Go module.
It contains two valid metadata pages (transactions 2 and 3), inline buckets,
a multi-page bucket with 2048 keys, empty binary-key values, and image target
digests in separate `moby` and `k8s.io` namespaces.

SHA-256: `7c8dc7685d6d6cb3b4f07ff199c81f03049c9ebd29764c685a907b958c28b015`.
Size: 262144 bytes; page size: 4096 bytes; active transaction: 3.

API reference: [bbolt v1.4.3](https://github.com/etcd-io/bbolt/blob/v1.4.3/README.md).
