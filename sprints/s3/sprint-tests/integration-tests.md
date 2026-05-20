# Sprint 3 Integration Tests

The reference-examples test file IS the integration surface for new
Carbide functions. The 14 new tests added this sprint parse + evaluate
real Carbide expressions against the engine, which exercises the
lexer → parser → eval → function-registry path end-to-end.

No additional integration harnesses added.
