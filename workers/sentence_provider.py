"""Optional neural embeddings/reranker. Models must already exist locally.
No trust_remote_code, no model downloads, no pickle input. Requires sentence-transformers.
The host selects the reviewed model directory and response model identifier; this worker does not load project Python modules.
"""
import json,sys,os,pathlib
os.environ['HF_HUB_OFFLINE']='1'
os.environ['TRANSFORMERS_OFFLINE']='1'

def main():
    req=json.load(sys.stdin);payload=req['payload']
    selected=sys.argv[1] if len(sys.argv)>1 else os.environ.get('CI_MODEL_PATH')
    if not selected:raise ValueError('Pass model directory as argument or CI_MODEL_PATH')
    model_path=pathlib.Path(selected).resolve(strict=True)
    if not model_path.is_dir():raise ValueError('Model path must be a local directory')
    # Refuse custom-code model modules and unsafe weight serialization by policy.
    if any(model_path.rglob('*.py')) or any(model_path.rglob('*.pkl')) or any(model_path.rglob('*.bin')):
        raise ValueError('Use audited safetensors-only models without custom Python modules')
    from sentence_transformers import SentenceTransformer,CrossEncoder
    result={'schema':1,'model':req['model'],'actualCost':0,'tokenMeasurement':'not_reported_by_local_encoder'}
    if req['operation']=='embed':
        model=SentenceTransformer(str(model_path),local_files_only=True,trust_remote_code=False,device='cpu')
        result['vectors']=model.encode(payload['texts'],batch_size=16,normalize_embeddings=True,show_progress_bar=False).tolist()
    elif req['operation']=='rerank':
        model=CrossEncoder(str(model_path),local_files_only=True,trust_remote_code=False,device='cpu')
        scores=model.predict([(payload['query'],d['text']) for d in payload['candidates']],show_progress_bar=False)
        result['items']=[{'entityId':d['entityId'],'score':float(s)} for d,s in zip(payload['candidates'],scores)]
    else:raise ValueError('This local worker supports embed and rerank, not text generation')
    print(json.dumps(result))
try:main()
except Exception as e:print(json.dumps({'error':type(e).__name__,'message':str(e)[:1000]}));sys.exit(2)
