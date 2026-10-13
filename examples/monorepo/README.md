# Configured TypeScript example

A solution config references two packages. Each package has its own compilation roots; both inherit the base config. The app uses a path alias into the core project.

From the Code Intelligence repository root:

```sh
node dist/cli.js index --root examples/monorepo --timeout 120000
node dist/cli.js find subtotal --root examples/monorepo --timeout 120000
node dist/cli.js verify --root examples/monorepo --timeout 120000
```

Use a returned entity ID to prepare a rename with `ci_change_prepare`. The plan should change the core declaration and the app import/use. It must not modify the original files until explicitly applied. Source-based project-reference navigation does not execute `tsc --build` or certify generated declaration compatibility.
