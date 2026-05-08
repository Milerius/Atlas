# atlas-core

`atlas-core` contains the first Atlas blockchain primitives:

- typed ids
- raw amount model
- chain and network models
- asset group, instrument, and instance models
- split chain and asset registry documents
- registry validation and lookup
- signing request and response boundary
- chain service trait
- mock EVM service for boundary tests

Execution must always use `AssetInstance`. `AssetGroup` and `AssetInstrument`
are for display, search, aggregation, and routing layers.
