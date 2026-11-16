# 0xSero's sparkinfer image with xgrammar fixed at build time. The recipe's own boot fix runs
# `pip install -U "xgrammar>=0.2.4"` on every start: today that also upgrades apache-tvm-ffi, and tilelang 0.1.9
# (which the DeepSeek V4 head kernel needs) then fails to import. Pinning xgrammar with --no-deps keeps
# apache-tvm-ffi 0.1.10 and transformers 5.13.1 as the image ships them.
ARG BASE_IMAGE
FROM ${BASE_IMAGE}
RUN /opt/runtime-venv/bin/pip install --no-cache-dir --no-deps "xgrammar==0.2.6" \
 && /opt/runtime-venv/bin/python -c "import xgrammar; assert hasattr(xgrammar, 'normalize_tool_choice')"
