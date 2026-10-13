// Static go/types provider. It compiles no user program and never downloads modules.
// Workspace modules and GOROOT source are resolved; unavailable dependencies fail closed for rename.
package main
import (
 "encoding/json"; "fmt"; "go/ast"; "go/build"; "go/importer"; "go/parser"; "go/token"; "go/types"; "os"; "path/filepath"; "runtime"; "sort"; "strings"; "unicode"; "unicode/utf8"
)
type Request struct { Root, Path, Operation, NewName string; Byte int; Files map[string]string }
type Location struct { Path string `json:"path"`; Name string `json:"name"`; StartByte int `json:"startByte"`; EndByte int `json:"endByte"`; StartLine int `json:"startLine"`; EndLine int `json:"endLine"` }
type Edit struct { Path string `json:"path"`; Text string `json:"text"` }
type Group struct { dir, pkgpath, name string; files []*ast.File; info *types.Info; pkg *types.Package; busy bool }
type Analyzer struct { req Request; fset *token.FileSet; groups map[string]*Group; imports map[string]string; errors []string; standard types.Importer; content map[string]string; overlays map[string]string }
func (a *Analyzer) Import(path string)(*types.Package,error){
 if key,ok:=a.imports[path];ok {g:=a.groups[key];if g.busy{return nil,fmt.Errorf("import cycle: %s",path)};return a.check(g)}
 // Never use GOPATH/module cache or query a proxy. Only Go's standard library is permitted.
 clean:=filepath.Clean(filepath.FromSlash(path)); if filepath.IsAbs(clean)||clean==".."||strings.HasPrefix(clean,".."+string(filepath.Separator)){return nil,fmt.Errorf("invalid import")}
 if strings.Contains(strings.Split(path,"/")[0],"."){return nil,fmt.Errorf("uncaptured dependency: %s",path)}
 if _,e:=os.Stat(filepath.Join(runtime.GOROOT(),"src",clean));e!=nil{return nil,fmt.Errorf("unavailable dependency: %s",path)}
 return a.standard.Import(path)
}
func (a *Analyzer) check(g *Group)(*types.Package,error){
 if g.pkg!=nil{return g.pkg,nil};g.busy=true;defer func(){g.busy=false}()
 g.info=&types.Info{Defs:map[*ast.Ident]types.Object{},Uses:map[*ast.Ident]types.Object{},Selections:map[*ast.SelectorExpr]*types.Selection{},Types:map[ast.Expr]types.TypeAndValue{}}
 cfg:=types.Config{Importer:a,Error:func(e error){if len(a.errors)<500{a.errors=append(a.errors,e.Error())}},Sizes:types.SizesFor("gc",runtime.GOARCH)}
 p,e:=cfg.Check(g.pkgpath,a.fset,g.files,g.info);g.pkg=p;return p,e
}
func newAnalyzer(req Request,overlays map[string]string)(*Analyzer,error){
 a:=&Analyzer{req:req,fset:token.NewFileSet(),groups:map[string]*Group{},imports:map[string]string{},content:map[string]string{},overlays:overlays,errors:[]string{}}
 a.standard=importer.ForCompiler(a.fset,"source",nil)
 modules:=map[string]string{}
 for path:=range req.Files{if filepath.Base(path)=="go.mod"{raw,e:=os.ReadFile(filepath.Join(req.Root,filepath.FromSlash(path)));if e!=nil{return nil,e};for _,line:=range strings.Split(string(raw),"\n"){f:=strings.Fields(line);if len(f)>=2&&f[0]=="module"{modules[filepath.ToSlash(filepath.Dir(path))]=strings.Trim(f[1],"\"");break}}}}
 paths:=[]string{};for path:=range req.Files{if strings.HasSuffix(path,".go"){paths=append(paths,path)}};sort.Strings(paths)
 for _,path:=range paths{
  dir:=filepath.ToSlash(filepath.Dir(path));full:=filepath.Join(req.Root,filepath.FromSlash(path))
  match,e:=build.Default.MatchFile(filepath.Dir(full),filepath.Base(full));if e!=nil{a.errors=append(a.errors,e.Error());continue};if !match{continue}
  raw,e:=os.ReadFile(full);if e!=nil{return nil,e};source:=string(raw);if v,ok:=overlays[path];ok{source=v};a.content[path]=source
  file,e:=parser.ParseFile(a.fset,full,source,parser.AllErrors|parser.ParseComments);if e!=nil{a.errors=append(a.errors,e.Error());if file==nil{continue}}
  pkgpath:="workspace/"+dir;best:=-1
  for prefix,module:=range modules{if prefix=="."||dir==prefix||strings.HasPrefix(dir,prefix+"/"){if len(prefix)>best{suffix:=strings.TrimPrefix(strings.TrimPrefix(dir,prefix),"/");if prefix=="."{suffix=dir};pkgpath=module;if suffix!=""&&suffix!="."{pkgpath+="/"+suffix};best=len(prefix)}}}
  key:=dir+"#"+file.Name.Name;g:=a.groups[key];if g==nil{g=&Group{dir:dir,pkgpath:pkgpath,name:file.Name.Name};a.groups[key]=g}
  if !strings.HasSuffix(file.Name.Name,"_test"){a.imports[pkgpath]=key}else{g.pkgpath=pkgpath+"_test"}
  g.files=append(g.files,file)
 }
 keys:=[]string{};for key:=range a.groups{keys=append(keys,key)};sort.Strings(keys);for _,key:=range keys{a.check(a.groups[key])};return a,nil
}
func (a *Analyzer) location(id *ast.Ident)*Location {p:=a.fset.Position(id.Pos());rel,e:=filepath.Rel(a.req.Root,p.Filename);if e!=nil{return nil};path:=filepath.ToSlash(rel);if _,ok:=a.req.Files[path];!ok{return nil};raw:=a.content[path];if p.Offset<0||p.Offset+len(id.Name)>len(raw)||raw[p.Offset:p.Offset+len(id.Name)]!=id.Name{return nil};return &Location{path,id.Name,p.Offset,p.Offset+len(id.Name),p.Line,p.Line}}
func validIdentifier(s string)bool{if s==""||token.Lookup(s).IsKeyword()||s=="_"{return false};for i,r:=range s{if !(r=='_'||unicode.IsLetter(r)||i>0&&unicode.IsDigit(r)){return false}};return utf8.ValidString(s)}
func run(req Request)(map[string]any,error){
 a,e:=newAnalyzer(req,nil);if e!=nil{return nil,e}
 var target types.Object
 for _,g:=range a.groups{if g.info==nil{continue};for _,m:=range []map[*ast.Ident]types.Object{g.info.Defs,g.info.Uses}{for id,obj:=range m{loc:=a.location(id);if loc!=nil&&loc.Path==req.Path&&req.Byte>=loc.StartByte&&req.Byte<loc.EndByte&&obj!=nil{if target!=nil&&target!=obj{return nil,fmt.Errorf("ambiguous semantic target")};target=obj}}}}
 if target==nil{return nil,fmt.Errorf("no resolved identifier at byte position")}
 items:=[]Location{};seen:=map[string]bool{};edits:=[]Edit{}
 for _,g:=range a.groups{if g.info==nil{continue};for i,m:=range []map[*ast.Ident]types.Object{g.info.Defs,g.info.Uses}{if req.Operation=="definition"&&i==1{continue};for id,obj:=range m{if obj!=target{continue};loc:=a.location(id);if loc==nil{continue};key:=fmt.Sprintf("%s:%d",loc.Path,loc.StartByte);if !seen[key]{seen[key]=true;items=append(items,*loc)}}}}
 sort.Slice(items,func(i,j int)bool{if items[i].Path==items[j].Path{return items[i].StartByte<items[j].StartByte};return items[i].Path<items[j].Path})
 if req.Operation=="rename"{
  if !validIdentifier(req.NewName){return nil,fmt.Errorf("new name is not a Go identifier")};if len(a.errors)>0{return nil,fmt.Errorf("rename requires a type-error-free captured Go package set: %s",a.errors[0])}
  if !target.Pos().IsValid(){return nil,fmt.Errorf("cannot rename an external or builtin object")}
  updated:=map[string]string{};byFile:=map[string][]Location{};for _,l:=range items{byFile[l.Path]=append(byFile[l.Path],l)}
  for path,ls:=range byFile{source:=a.content[path];for i:=len(ls)-1;i>=0;i--{l:=ls[i];source=source[:l.StartByte]+req.NewName+source[l.EndByte:]};updated[path]=source;edits=append(edits,Edit{path,source})}
  next,e:=newAnalyzer(req,updated);if e!=nil{return nil,e};if len(next.errors)>0{return nil,fmt.Errorf("rename introduces Go type errors: %s",next.errors[0])}
  // Type checking alone does not catch a silent shadowing/rebinding. Every old
  // definition/reference must still refer to the same new declaration object.
  var rebound types.Object
  expected:=map[string]bool{}
  for path,ls:=range byFile{delta:=0;for _,l:=range ls{expected[fmt.Sprintf("%s:%d",path,l.StartByte+delta)]=true;delta+=len(req.NewName)-len(l.Name)}}
  matched:=0;definitions:=0
  for _,g:=range next.groups{if g.info==nil{continue};for i,m:=range []map[*ast.Ident]types.Object{g.info.Defs,g.info.Uses}{for id,obj:=range m{l:=next.location(id);if l==nil||!expected[fmt.Sprintf("%s:%d",l.Path,l.StartByte)]{continue};if obj==nil||id.Name!=req.NewName{return nil,fmt.Errorf("rename lost a binding")};if rebound!=nil&&rebound!=obj{return nil,fmt.Errorf("rename would silently rebind a reference")};rebound=obj;matched++;if i==0{definitions++}}}}
  if matched!=len(expected)||definitions!=1{return nil,fmt.Errorf("rename has an incomplete or ambiguous declaration set")}
 }
 return map[string]any{"provider":"go/types","version":runtime.Version(),"items":items,"edits":edits,"diagnostics":a.errors,"exhaustive":false,"sourceExecuted":false,"limitations":[]string{"Type analysis uses the host GOOS/GOARCH, captured workspace modules and GOROOT; generated code and missing external modules are not downloaded.","Reflection, string-based lookup, cross-platform build-tag variants and downstream users outside the snapshot are not covered."}},nil
}
func main(){var req Request;if e:=json.NewDecoder(os.Stdin).Decode(&req);e!=nil{fmt.Fprintln(os.Stderr,e);os.Exit(2)};root,e:=filepath.Abs(req.Root);if e!=nil{os.Exit(2)};req.Root=root;result,e:=run(req);if e!=nil{json.NewEncoder(os.Stdout).Encode(map[string]any{"error":e.Error()});os.Exit(2)};json.NewEncoder(os.Stdout).Encode(result)}
