#!/bin/bash
# The six games on Sonnet, chained: step writes the next prompt, Sonnet writes its chronicle, which
# goes into the book before the next prompt is built. Three lanes of two games.
SP=/tmp/claude-1000/-home-deck-Downloads-engine13/fe9ae962-d66a-457a-ba4f-5cb2a4e827a4/scratchpad
D=$SP/cmp2/claude-sonnet-5-5
game() { # world seed first
  while true; do
    out=$($SP/cp_step step $1 $2 $3 10 $D 2>/dev/null)
    [ "$out" = "DONE" ] && { echo "sonnet $1 s$2 DONE"; return; }
    set -- $1 $2 $3 $out
    ok=0
    for try in 1 2 3; do $SP/sonnet_one.sh $5 $6 && { ok=1; break; }; sleep 30; done
    [ $ok -eq 1 ] || { echo "sonnet $1 s$2 FAILED at $6"; return; }
  done
}
mkdir -p $D
( game none 0 0; game aggressive 0 0 ) &
( game none 9 10; game aggressive 7 15 ) &
( game none 2 17; game aggressive 9 23 ) &
wait
