This package exists only for testing. The `build.rs` script should take care of compiling and linking the cgo bindings, but if you want to build it yourself (e.g. to inspect the generated header file), run:

```bash
go build -buildmode=c-archive -o out.a main.go
```
