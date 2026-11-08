from pathlib import Path
temp=Path.home()/'AppData/Local/Temp';source=(temp/'ci-falkor-vm-baseline-r4.py').read_text(encoding='utf-8-sig').replace("base/'vm-baseline-r4'","base/'generated-live-r1'").replace("~owned:* +@all", "~owned:* ~yoke:native:v1:* +@all")
start=source.index("    text='quoted");end=source.index("    result={",start);source=source[:start]+(temp/'ci-falkor-generated-live-block.txt').read_text()+source[end:]
source=source.replace("'unicodePasswordAuthentication':True", "'unicodePasswordAuthentication':True,'compiledNativeFixedCommands':True,'nativeTransportTested':False,'atomicFreshGraphWatchExec':True,'allOriginalRowsEvidenceAndHeaderExact':True,'parallelDirectionsAndUnresolvedTargetsExact':True,'sameRevisionReplayIdempotent':True,'actualCountConflictRollsBackCounter':True")
(temp/'ci-falkor-generated-live.py').write_text(source,encoding='utf-8')
