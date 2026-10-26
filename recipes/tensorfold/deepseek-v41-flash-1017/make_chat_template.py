"""make_chat_template.py <original> <output>: the checkpoint's chat template -> one this TensorFold build can compile.
prepare.sh runs it on the template that ships with the weights; it asserts every change applies exactly once, so a
different template fails loudly instead of being half converted."""
import re, sys

SRC, DST = sys.argv[1], sys.argv[2]
src = open(SRC, encoding='utf-8').read()
R = [
 (r'dict(stack[-1]["v"], **{stack[-1]["k"]: val})', r'dict((stack[-1]["v"].items() | list) + [[stack[-1]["k"], val]])'),
 (r'dict(schema.d, **{k: v})', r'dict((schema.d.items() | list) + [[k, v]])'),
 (r'{%- set JSON_LOADS = "fromjson" is filter or JSON_PROBE.copy() is sameas JSON_PROBE -%}', r'{%- set JSON_LOADS = false -%}'),
 (r'{%- if content is string and IMG in content -%}', r'{%- if content is string and IMG in content and msg.role not in ["user", "tool"] -%}'),
 # Tuned: maximum reasoning by default; a request can still ask for another level
 (r'{%- set reasoning_effort = "high" -%}{%- endif -%}', r'{%- set reasoning_effort = "max" -%}{%- endif -%}'),
 # user turns (user text and tool results merged into blocks): the placeholder in their text is the server's own image
 ('{%- if IMG in (b.text or "") -%}{{- raise_exception("Text block contains image placeholder " ~ IMG ~ ". Images should be separate content blocks.") -}}{%- endif -%}\n'
  '                {%- set up.parts = up.parts + [b.text or ""] -%}', '{%- set up.parts = up.parts + [b.text or ""] -%}'),
 ('{%- if IMG in (s.text or "") -%}{{- raise_exception("Text block contains image placeholder " ~ IMG ~ ". Images should be separate content blocks.") -}}{%- endif -%}\n'
  '                            {%- set sub.parts', '{%- set sub.parts'),
]
out = src
for a, b in R:
    assert out.count(a) == 1, a
    out = out.replace(a, b)
# The 5 fromjson calls (only reachable with JSON_LOADS) raise instead
out, n = re.subn(r'(\{%- set [\w.]+ = )[^\n]*?\| fromjson( -%\})', r'\1raise_exception("fromjson")\2', out)
assert n == 5, n
note = ('{#- Ambassador Local: adapted to the TensorFold build that serves it (bertholomus/TensorFold deepseek-v41-zig @ 6366d2c).\n'
        '    Its Jinja engine has no fromjson filter and no star-args calls: JSON_LOADS is fixed to false, the fromjson sites\n'
        '    (only reachable with JSON_LOADS) raise instead, and dict(x, double-star {k: v}) is written\n'
        '    dict((x.items() | list) + [[k, v]]). The plain-Jinja JSON decoder left in place needs int/float/format, which this\n'
        '    engine lacks too, but its server already hands tool-call arguments over as objects (messages.zig toolArguments),\n'
        '    so the decoder is never reached. The server also sends images as the placeholder inside user/tool text (messages.zig\n'
        '    normalizeImages, which escapes typed ones), so the placeholder in string content is refused only for other roles.\n'
        '    Default reasoning effort is "max" (budget 100) instead of "high": a request that names an effort still gets it. -#}\n')
i = out.index('{%- set IMG'); out = out[:i] + note + out[i:]
code = re.sub(r'\{#.*?#\}', '', out, flags=re.S)
assert '| fromjson' not in code and '**' not in code
open(DST, 'w', encoding='utf-8', newline='\n').write(out)
print('ok', len(out))
