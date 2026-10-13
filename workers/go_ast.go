// Code Intelligence: stdlib-only Go syntax extraction, no project execution.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"runtime"
	"strconv"
	"strings"
)

type File struct {
	Path    string `json:"path"`
	Content string `json:"content"`
}
type Input struct {
	Files []File `json:"files"`
}
type Dict map[string]any

func expression(fset *token.FileSet, n ast.Node) string {
	var b bytes.Buffer
	_ = printer.Fprint(&b, fset, n)
	return b.String()
}
func main() {
	var input Input
	if err := json.NewDecoder(os.Stdin).Decode(&input); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	results := []Dict{}
	for _, f := range input.Files {
		entities, observations, diagnostics := []Dict{}, []Dict{}, []Dict{}
		fs := token.NewFileSet()
		tree, err := parser.ParseFile(fs, f.Path, f.Content, parser.AllErrors|parser.ParseComments)
		producer := "go-parser@" + runtime.Version()
		if err != nil {
			diagnostics = append(diagnostics, Dict{"path": f.Path, "code": "GO_SYNTAX", "severity": "error", "message": err.Error()})
		}
		span := func(n ast.Node) Dict {
			a, b := fs.Position(n.Pos()), fs.Position(n.End())
			return Dict{"startByte": a.Offset, "endByte": b.Offset, "startLine": a.Line, "endLine": b.Line}
		}
		merge := func(a, b Dict) Dict {
			for k, v := range b {
				a[k] = v
			}
			return a
		}
		names := map[string][]Dict{}
		add := func(n ast.Node, name *ast.Ident, kind, qualified string) {
			r := span(n)
			key := fmt.Sprintf("%s#%d:%s:%s", f.Path, r["startByte"], kind, qualified)
			signature := expression(fs, n)
			if i := strings.Index(signature, "\n"); i >= 0 {
				signature = signature[:i]
			}
			if len(signature) > 400 {
				signature = signature[:400]
			}
			entity := merge(Dict{"key": key, "path": f.Path, "name": name.Name, "qualifiedName": qualified, "kind": kind, "language": "go", "signature": signature, "producer": producer, "parentKey": "file:" + f.Path, "nameStartByte": fs.Position(name.Pos()).Offset, "nameEndByte": fs.Position(name.End()).Offset}, r)
			entities = append(entities, entity)
			names[name.Name] = append(names[name.Name], entity)
			observations = append(observations, merge(Dict{"sourceKey": "file:" + f.Path, "targetKey": key, "relation": "contains", "resolution": "syntactic", "path": f.Path, "producer": producer}, span(n)))
		}
		if tree != nil {
			for _, d := range tree.Decls {
				switch n := d.(type) {
				case *ast.FuncDecl:
					kind, q := "function", n.Name.Name
					if n.Recv != nil && len(n.Recv.List) > 0 {
						kind = "method"
						q = expression(fs, n.Recv.List[0].Type) + "." + n.Name.Name
					}
					add(n, n.Name, kind, q)
				case *ast.GenDecl:
					for _, s := range n.Specs {
						switch spec := s.(type) {
						case *ast.TypeSpec:
							kind := "type"
							switch spec.Type.(type) {
							case *ast.StructType:
								kind = "struct"
							case *ast.InterfaceType:
								kind = "interface"
							}
							add(spec, spec.Name, kind, spec.Name.Name)
						case *ast.ValueSpec:
							for _, name := range spec.Names {
								kind := "variable"
								if n.Tok == token.CONST {
									kind = "constant"
								}
								add(spec, name, kind, name.Name)
							}
						case *ast.ImportSpec:
							label, _ := strconv.Unquote(spec.Path.Value)
							observations = append(observations, merge(Dict{"sourceKey": "file:" + f.Path, "targetLabel": label, "relation": "imports", "resolution": "unresolved", "path": f.Path, "producer": producer}, span(spec)))
						}
					}
				}
			}
			ast.Inspect(tree, func(n ast.Node) bool {
				call, ok := n.(*ast.CallExpr)
				if !ok {
					return true
				}
				source := "file:" + f.Path
				smallest := len(f.Content) + 1
				at := fs.Position(call.Pos()).Offset
				for _, e := range entities {
					start, end := e["startByte"].(int), e["endByte"].(int)
					if (e["kind"] == "function" || e["kind"] == "method") && start <= at && end >= at && end-start < smallest {
						source = e["key"].(string)
						smallest = end - start
					}
				}
				edge := merge(Dict{"sourceKey": source, "targetLabel": expression(fs, call.Fun), "relation": "calls", "resolution": "unresolved", "path": f.Path, "producer": producer}, span(call.Fun))
				if ident, ok := call.Fun.(*ast.Ident); ok && len(names[ident.Name]) == 1 {
					edge["targetKey"] = names[ident.Name][0]["key"]
					edge["resolution"] = "heuristic"
					delete(edge, "targetLabel")
				}
				observations = append(observations, edge)
				return true
			})
		}
		results = append(results, Dict{"entities": entities, "observations": observations, "diagnostics": diagnostics, "limitations": []string{"Go syntax and same-file name candidates are not go/types or runtime proofs; external modules remain unresolved."}})
	}
	_ = json.NewEncoder(os.Stdout).Encode(Dict{"results": results, "producer": "go-parser@" + runtime.Version()})
}
