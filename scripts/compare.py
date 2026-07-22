#!/usr/bin/env python3
"""Compare feature annotations against StoryScope's published values, and
measure human-vs-AI separation per rater.

Answers two questions from JSONL annotations (passage, rater, run, values):
  1. Concordance: for each rater, how often does its value match StoryScope's
     published value on the same story/feature? Reported as agreement corrected
     for chance is overkill at pilot n; we report exact and (for numeric)
     within-one rates.
  2. Separation: for each rater, per feature, the total-variation distance
     between the human-story value distribution and the AI-story value
     distribution. A rater that separates human from AI *more* than StoryScope's
     own published values did is finding signal the weaker annotator missed --
     the whole question. Passage names encode class: 'human-*' vs 'ai-*'.

Taxonomy JSON gives each feature's type so numeric distance is ordered.
"""
import json, sys, collections, argparse, statistics

def load_jsonl(path):
    out=[]
    with open(path) as f:
        for line in f:
            line=line.strip()
            if line: out.append(json.loads(line))
    return out

def load_taxonomy(path):
    tax={}
    t=json.load(open(path))
    for dim,dv in t["feature_taxonomy"].items():
        for asp,av in dv["aspects"].items():
            for f in av["features"]: tax[f["id"]]=f
    return tax

def norm_val(tax, fid, v):
    """Canonicalize a value to something comparable across raters.
    Numeric (scale/ordinal) -> int rank; categorical/binary -> str; multi -> frozenset."""
    f=tax[fid]; typ=f["type"]
    if v is None: return None
    if typ=="multi_select":
        if isinstance(v,list): return frozenset(v)
        return frozenset(x for x in str(v).split("|") if x and x!="n/a")
    if typ=="scale":
        try: return ("n", int(v))
        except: return None
    if typ=="ordinal":
        if isinstance(v,int): return ("n", v)
        try: return ("n", f["values"].index(v)+1)   # label -> rank
        except: return None
    return ("c", str(v))  # categorical/binary

def dist_num(a,b): 
    d=abs(a[1]-b[1]); return d

def cls(passage):
    return "human" if passage.startswith("human") else "ai"

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--annotations", nargs="+", required=True)
    ap.add_argument("--reference", required=True, help="storyscope published jsonl")
    ap.add_argument("--taxonomy", required=True)
    args=ap.parse_args()

    tax=load_taxonomy(args.taxonomy)
    ref=load_jsonl(args.reference)
    # ref: passage -> {fid: normval}
    refmap={}
    for r in ref:
        refmap[r["passage"]]={fid: norm_val(tax,fid,v) for fid,v in r["values"].items()}
    features=sorted({fid for r in ref for fid in r["values"]})
    passages=sorted(refmap)
    humans=[p for p in passages if cls(p)=="human"]
    ais=[p for p in passages if cls(p)=="ai"]
    print(f"{len(passages)} passages ({len(humans)} human, {len(ais)} ai), {len(features)} features\n")

    raters={}
    for path in args.annotations:
        for a in load_jsonl(path):
            if a["run"] not in (0,1): continue
            raters.setdefault(a["rater"],{})[a["passage"]]={fid: norm_val(tax,fid,v) for fid,v in a["values"].items()}

    # --- Concordance vs storyscope published ---
    print("=== CONCORDANCE with StoryScope published values ===")
    print(f"{'rater':14s} {'exact':>7} {'within1':>8}  (over matched story/feature cells)")
    for rname,rmap in raters.items():
        exact=tot=near=numtot=0
        for p in passages:
            for fid in features:
                rv=refmap[p].get(fid); ov=rmap.get(p,{}).get(fid)
                if rv is None or ov is None: continue
                tot+=1
                if rv==ov: exact+=1
                if isinstance(rv,tuple) and rv[0]=="n" and isinstance(ov,tuple) and ov[0]=="n":
                    numtot+=1
                    if dist_num(rv,ov)<=1: near+=1
        w1 = f"{near/numtot:.0%}" if numtot else "  -"
        print(f"{rname:14s} {exact/tot:7.0%} {w1:>8}  (n={tot})")

    # --- Separation: human vs AI, per rater, mean over features ---
    print("\n=== SEPARATION human-vs-AI (mean total-variation distance over features) ===")
    print("higher = better human/AI separation. 'storyscope' = their own published values.")
    def tvd_for(valmap, fids):
        # valmap: passage -> {fid:normval}; returns per-feature TVD then mean
        out=[]
        for fid in fids:
            hc=collections.Counter(); ac=collections.Counter()
            for p in humans:
                v=valmap.get(p,{}).get(fid)
                if v is None: continue
                if isinstance(v,frozenset):
                    for tok in (v or {"<none>"}): hc[tok]+=1
                else: hc[v]+=1
            for p in ais:
                v=valmap.get(p,{}).get(fid)
                if v is None: continue
                if isinstance(v,frozenset):
                    for tok in (v or {"<none>"}): ac[tok]+=1
                else: ac[v]+=1
            ht=sum(hc.values()); at=sum(ac.values())
            if ht==0 or at==0: continue
            keys=set(hc)|set(ac)
            tvd=0.5*sum(abs(hc[k]/ht-ac[k]/at) for k in keys)
            out.append((fid,tvd))
        return out
    allmaps={"storyscope":refmap, **raters}
    means={}
    perfeat={}
    for name,vm in allmaps.items():
        pf=tvd_for(vm, features); perfeat[name]=dict(pf)
        means[name]=statistics.mean(t for _,t in pf) if pf else 0
    for name in sorted(means, key=lambda n:-means[n]):
        print(f"  {name:14s} mean TVD {means[name]:.3f} over {len(perfeat[name])} features")

    # Where do the strong raters separate MORE than storyscope did?
    print("\n=== features where a strong rater beats storyscope's separation by >=0.15 ===")
    ssf=perfeat.get("storyscope",{})
    for rname in raters:
        rf=perfeat[rname]
        gains=sorted(((rf[f]-ssf.get(f,0), f) for f in rf if f in ssf), reverse=True)
        big=[(g,f) for g,f in gains if g>=0.15]
        print(f"\n{rname}: {len(big)} feature(s)")
        for g,f in big[:12]:
            print(f"   +{g:.2f}  {f:14s} {tax[f]['name'][:44]}")

if __name__=="__main__": main()
