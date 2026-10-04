# ostrov's completion for fish (3.4 on), answered by the running ostrov (`ostrov complete WORD...`: a line a
# candidate, its description after a tab); nothing while none runs.

complete -c ostrov -f -a '(ostrov complete (commandline -opc)[2..] "$(commandline -ct)" 2>/dev/null)'
