use std::process::Command;
#[test]
fn frame_scheduler_completes_once_when_animation_frames_starve_or_race() {
    let html = include_str!("../src/explorer.html");
    let body = html
        .split_once("function scheduleFrame(")
        .unwrap()
        .1
        .split_once("function startLayout(")
        .unwrap()
        .0;
    let scheduler = format!("function scheduleFrame({body}");
    let script = format!(
        r#"{scheduler}
const assert=require('node:assert/strict');
for(const first of ['timer','frame','cancel']){{
 let timer,frame,calls=0,cause;const cleared=[],cancelled=[];
 global.setTimeout=fn=>{{timer=fn;return 99;}};global.clearTimeout=id=>cleared.push(id);
 global.requestAnimationFrame=fn=>{{frame=fn;return 7;}};global.cancelAnimationFrame=id=>cancelled.push(id);
 const cancel=scheduleFrame(value=>{{calls++;cause=value;}});
 if(first==='cancel')cancel();else if(first==='timer')timer();else frame();
 timer();frame();timer();frame();
 assert.equal(calls,first==='cancel'?0:1,'scheduler callback count for '+first);
 if(first!=='cancel')assert.equal(cause,first==='timer','winner type for '+first);
 assert(cleared.includes(99),'pending timer cancelled');assert(cancelled.includes(7),'pending animation frame cancelled');
}}
"#
    );
    let result = Command::new("node")
        .args(["-e", &script])
        .output()
        .expect("Host Node runtime required for offline explorer scheduler checks");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn first_throttled_timer_finishes_bounded_layout_and_preserves_all_observations() {
    let html = include_str!("../src/explorer.html");
    let body = html
        .split_once("function startLayout(")
        .unwrap()
        .1
        .split_once("function fit(")
        .unwrap()
        .0;
    let layout = format!("function startLayout({body}");
    let script = format!(
        r#"
const assert=require('node:assert/strict');
const FORCE_STEPS=48;
for(const [nodeCount,edgeCount] of [[100,0],[101,5000]]){{
 let layoutVersion=0,layoutRunning=false,positions=new Map(),draws=0,status='',pending=[];
 const shown=Array.from({{length:nodeCount}},(_,i)=>({{id:String(i)}}));
 const visibleEdges=Array.from({{length:edgeCount}},(_,i)=>({{id:String(i),source:String(i%nodeCount),target:String((i+1)%nodeCount)}}));
 const unchanged=JSON.stringify(visibleEdges),image={{disabled:false}};
 const $=id=>id==='layout'?{{value:'force'}}:image;
 function fit(){{}}function scheduleDraw(){{}}function updateStatus(value){{status=value||'ready';}}
 function renderNow(){{draws++;image.disabled=layoutRunning;}}
 function scheduleFrame(callback){{pending.push(callback);}}
 {layout}
 startLayout();assert.equal(pending.length,1);assert.equal(layoutRunning,true);assert.equal(image.disabled,true);
 pending.shift()(true);
 assert.equal(layoutRunning,false);assert.equal(pending.length,0,'no nested timer chain after timer winner');
 assert.equal(draws,1,'synchronous final rendering');assert.equal(image.disabled,false);assert.equal(status,'ready');
 assert.equal(positions.size,nodeCount);assert([...positions.values()].every(p=>Number.isFinite(p.x)&&Number.isFinite(p.y)));
 assert.equal(JSON.stringify(visibleEdges),unchanged,'layout sampling must preserve all observations');
}}
"#
    );
    let result = Command::new("node")
        .args(["-e", &script])
        .output()
        .expect("Host Node runtime required for offline explorer layout checks");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
