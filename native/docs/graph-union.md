# Graphify graph union

`yoke graph union-merge base.json current.json other.json` previews a
node-link union of two Graphify graph files. Each side supplies nodes and
either edges or links. The output uses links with unique multigraph keys
and keeps incoming edge keys as yokeOriginalEdgeKey. It preserves the
direction marked by _src and _tgt, parallel observations, and the maximum
multiplicity of exact rows on either side. Node and graph attributes from
the other side take precedence for shared keys. The union keeps unresolved
edges without creating connected nodes for them.

The common ancestor is admitted as an ordinary bounded file. The explicit
union is based on the two branch versions, so deleted nodes may still be
present in the result. Refreshing a generated graph from the merged source
is a separate step.

The local Git driver command uses an explicitly selected native binary.

```sh
git config --local merge.yoke-graphify.driver "'/absolute/path/to/yoke' graph union-merge --write-git-current %O %A %B"
```

Assign merge=yoke-graphify in .gitattributes to each tracked generated graph
that should use the driver. Git passes three distinct scratch-file basenames.
The driver validates both graph inputs, stages the complete result, checks
the original current file bytes again and replaces Git's %A scratch file.
An invalid input, byte limit or publication failure causes a merge conflict.
