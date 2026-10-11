#!/bin/bash
# One chronicle by Sonnet through the subscription (claude -p), split as the product's Anthropic path
# splits it (stream_narrative_anthropic): system = before «=== НАРРАТИВНЫЕ ИНСТРУКЦИИ ===», user = the rest.
# $1 prompt file, $2 text file to write. Returns non-zero and writes nothing on an error.
SP=/tmp/claude-1000/-home-deck-Downloads-engine13/fe9ae962-d66a-457a-ba4f-5cb2a4e827a4/scratchpad
sys=$(python3 -I -c "import sys;p=open(sys.argv[1]).read();i=p.find('=== НАРРАТИВНЫЕ ИНСТРУКЦИИ ===');print(p[:i].strip() if i>0 else p[:500])" "$1")
python3 -I -c "import sys;p=open(sys.argv[1]).read();i=p.find('=== НАРРАТИВНЫЕ ИНСТРУКЦИИ ===');sys.stdout.write(p[i:].strip() if i>0 else p)" "$1" > "$2.user"
cd $SP/empty_cwd
claude -p --model claude-sonnet-5-5 --system-prompt "$sys" --tools "" --no-session-persistence --output-format json < "$2.user" > "$2.json" 2> "$2.err"
rm -f "$2.user"
python3 -I -c "
import json,sys
d=json.load(open(sys.argv[1]))
if d.get('is_error') or not d.get('result','').strip(): sys.exit(1)
open(sys.argv[2],'w').write(d['result'])" "$2.json" "$2"
