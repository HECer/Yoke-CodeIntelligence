"""Separate analysis projection: canonical directed multiedges remain unchanged.
Explicit leiden requires official igraph/leidenalg; never relabel a fallback as Leiden.
"""
import json,sys,importlib.metadata

def main():
    value=json.load(sys.stdin);nodes=sorted(set(value['nodes']));indices={n:i for i,n in enumerate(nodes)};edges={}
    for e in value['edges']:
        if e[0] not in indices or e[1] not in indices or e[0]==e[1]:continue
        pair=tuple(sorted((e[0],e[1])));edges[pair]=edges.get(pair,0)+1
    algorithm=value.get('algorithm','louvain');seed=int(value.get('seed',0))
    if algorithm=='leiden':
        import igraph as ig,leidenalg as la
        g=ig.Graph(n=len(nodes),edges=[(indices[a],indices[b]) for a,b in edges],directed=False)
        part=la.find_partition(g,la.RBConfigurationVertexPartition,weights=list(edges.values()),seed=seed,n_iterations=-1)
        groups=[[nodes[i] for i in group] for group in part];version=importlib.metadata.version('leidenalg')
    elif algorithm=='louvain':
        import networkx as nx
        g=nx.Graph();g.add_nodes_from(nodes);g.add_weighted_edges_from((a,b,w) for (a,b),w in edges.items())
        groups=[sorted(c) for c in nx.community.louvain_communities(g,seed=seed)] if edges else [[n] for n in nodes]
        version=nx.__version__
    else:raise ValueError('Select leiden or louvain explicitly')
    print(json.dumps({'algorithm':algorithm,'version':version,'communities':sorted([sorted(g) for g in groups]),'projection':'undirected_weighted_for_clustering_only','seed':seed}))
try:main()
except Exception as e:print(json.dumps({'error':type(e).__name__,'message':str(e)[:500]}));sys.exit(2)
