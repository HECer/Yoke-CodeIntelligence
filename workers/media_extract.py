"""Native media extraction. No OCR, model calls, remote URLs, or invented captions."""
import sys,json,pathlib,wave,subprocess

def main():
    req=json.load(sys.stdin);path=pathlib.Path(req['path']);ext=path.suffix.lower();segments=[];meta={};warnings=[]
    if ext=='.pdf':
        import fitz
        doc=fitz.open(str(path))
        if doc.needs_pass:raise ValueError('Encrypted PDF requires an explicitly decrypted copy')
        if len(doc)>2000:raise ValueError('PDF page limit exceeded')
        for n,page in enumerate(doc):
            text=page.get_text('text')
            if len(text)>200000:raise ValueError('PDF page text limit exceeded')
            if text.strip():segments.append({'page':n+1,'text':text})
            else:warnings.append(f'Page {n+1} has no native text; OCR/vision not silently substituted')
        meta={'kind':'pdf','pages':len(doc),'provider':'pymupdf','version':fitz.VersionBind}
    elif ext in ['.png','.jpg','.jpeg','.webp']:
        from PIL import Image
        with Image.open(path) as im:meta={'kind':'image','format':im.format,'width':im.width,'height':im.height,'mode':im.mode}
        warnings.append('Image metadata only; visual content requires an explicitly configured vision provider')
    elif ext=='.wav':
        with wave.open(str(path),'rb') as w:meta={'kind':'audio','channels':w.getnchannels(),'sampleRate':w.getframerate(),'durationSeconds':w.getnframes()/w.getframerate()}
        warnings.append('Audio is not transcribed without a supplied transcript or configured ASR provider')
    elif ext in ['.mp4','.mov','.webm','.mp3','.flac','.ogg','.m4a']:
        proc=subprocess.run(['ffprobe','-v','error','-protocol_whitelist','file,pipe','-show_format','-show_streams','-of','json',str(path)],capture_output=True,timeout=20,check=True)
        value=json.loads(proc.stdout);meta={'kind':'audiovisual','durationSeconds':value.get('format',{}).get('duration'),'streams':[{k:s.get(k) for k in ['codec_type','codec_name','width','height','sample_rate']} for s in value.get('streams',[])]}
        warnings.append('Container metadata only; no automatic transcript or video interpretation')
    elif ext in ['.txt','.md','.vtt','.srt']:
        segments=[{'text':path.read_text(encoding='utf-8')}];meta={'kind':'transcript' if ext in ['.vtt','.srt'] else 'text','format':ext[1:]}
    else:raise ValueError('Unsupported native media format')
    if sum(len(s['text']) for s in segments)>4000000:raise ValueError('Extracted text exceeds four million characters')
    print(json.dumps({'metadata':meta,'segments':segments,'warnings':warnings,'modelCalls':0}))
try:main()
except Exception as e:print(json.dumps({'error':type(e).__name__,'message':str(e)[:1000]}));sys.exit(2)
