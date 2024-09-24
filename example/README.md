```
spin deps add registry --registry registry-by-karthik.fermyon.app --version ^0.1.0 component:markdown-renderer
spin deps generate-bindings -L rust -o src/bindings -c example
```
