import hashlib,json
def signature(text):
    chars=text.replace(' ','')
    shingles={chars} if len(chars)<3 else {chars[n:n+3] for n in range(len(chars)-2)}
    return [min(int(hashlib.sha256(json.dumps(['meaning-identity-minhash-v1',seed,s],ensure_ascii=False,separators=(',',':')).encode()).hexdigest()[:16],16) for s in shingles) for seed in range(64)]
a=signature('payment storage');b=signature('paymnt storage')
print(json.dumps({'firstFourA':a[:4],'firstFourB':b[:4],'matching':sum(x==y for x,y in zip(a,b)),'bands':sum(a[n:n+4]==b[n:n+4] for n in range(0,64,4))}))
