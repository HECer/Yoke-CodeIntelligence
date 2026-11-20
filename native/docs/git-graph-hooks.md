# Managed Git graph refresh

Install the native hooks into an explicitly selected Git repository.

~~~sh
yoke --root /host/project git graph-hooks install --trust-git --allow-hook-write
yoke --root /host/project git graph-hooks status
yoke --root /host/project git graph-hooks uninstall --allow-hook-write
~~~

The installer checks Git's effective hook directory. A path outside the
repository's ordinary .git directory needs explicit selection with
--hooks-dir. The directory must already exist. Install and uninstall
preserve foreign hook files. Existing foreign files at any of the three
managed names cause a conflict before activation.

The local receipt binds the repository root, hook location, selected Git
and Yoke binaries, and the source-scope snapshot. Supply --origin to use an
existing immutable capture, or let install create the initial native
capture. A repeat install with the same receipt resumes missing scripts.
The activation marker takes effect after all three scripts are in place.
Uninstall disables refresh before removing the registered scripts and
keeps the receipt and last graph for inspection or resumption.

Post-commit, branch checkout and post-merge refresh the Graphify node-link
file at .git/yoke-graph-hooks/graph.json. Its metadata carries the actual
HEAD commit, a source snapshot ID, a native graph hash, coverage and evidence.
The refresh reads verified Git objects and confirms HEAD before publishing
the complete graph. Path-only checkouts leave the current graph in place.
Working-tree edits and untracked files do not enter the committed graph.

The [Git merge driver](graph-union.md) can separately merge a tracked
Graphify artifact. The managed hooks publish a fresh local graph using the
committed source, including when a merged change deletes an old symbol.

Select a tracked graph with --union-graph to install the driver alongside
the hooks.

~~~sh
yoke --root /host/project git graph-hooks install --trust-git --allow-hook-write --union-graph graphs/graph.json
~~~

Yoke records the exact repository-relative path in the local receipt.
It adds the driver to the repository's Git configuration and adds a
literal assignment to .git/info/attributes. The starting merge attribute
must be unspecified. The driver verifies the path Git passes before
combining the scratch graph files. Install can resume a partial registration.
Uninstall removes the owned driver setting and attribute block and
preserves unrelated local Git configuration.
