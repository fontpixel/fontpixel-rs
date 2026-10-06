"""Every fontpixel command-line operation as an in-process call.

Each operation takes a request dict in the flat ``fontpixel_ops`` shape (keys
are the command-line argument ids in snake_case: ``inputs``, ``output``, ``to``,
``max_total_bytes``, ``otb_upm``, ...) and/or the same keys as keyword
arguments. Unknown keys, invalid choices, out-of-range numbers and violated
command-line rules (``each`` without ``to``, ``text`` together with ``all``,
...) raise :class:`fontpixel.InvalidInputError` with ``exit_code`` 2 and the
command-line parser's message.

Storage is chosen per call with ``files``:

* ``files=None`` (default) -- filesystem mode: request paths are real paths
  (``str`` or ``os.PathLike``), sidecars are discovered beside their primary
  file, sources are protected from being overwritten and every multi-output
  group is written transactionally, exactly as ``fontpixel`` does.
* ``files={"name.bdf": b"...", ...}`` or a :class:`fontpixel.ResourceSet` --
  memory mode: paths are keys into the named buffers (``bytes``, ``bytearray``
  or ``memoryview``); sidecars resolve beside their primary key and a key
  prefix acts as a directory. Nothing is read from or written to disk;
  ``result.outputs`` maps each prepared asset's key to its bytes and
  ``result.bundles`` each destination to its :class:`fontpixel.OutputBundle`.
* ``files=<resolver>`` -- memory mode over an object with ``entry(path)``
  (``None``, a file length or ``"directory"``), ``read(path) -> bytes`` and
  ``list(directory) -> list[str]``; an exception raised by a callback is
  re-raised from the operation.

Results (:class:`OperationResult`, :class:`BatchResult`) reproduce the command
line's standard output: ``result.json()`` is the ``--report json``/``--json``
document and ``result.text()`` the text output, byte for byte. Failures raise
the ``fontpixel`` exception classes with ``exit_code`` (the command-line exit
status), ``kind`` and, for strict-loss and reported errors, the full
``report``. The GIL is released while an operation runs, so calls from several
threads proceed in parallel.

Example::

    from fontpixel import ops
    result = ops.convert(inputs=["font.bdf"], output="font.pcf",
                         files={"font.bdf": open("font.bdf", "rb").read()})
    pcf = result.outputs["font.pcf"]
"""
from ._native import _ops

OperationResult = _ops.OperationResult
BatchResult = _ops.BatchResult

convert = _ops.convert
merge = _ops.merge
variants = _ops.variants
subset = _ops.subset
charset = _ops.charset
render = _ops.render
validate = _ops.validate
diff = _ops.diff
info = _ops.info
preview = _ops.preview
formats = _ops.formats
batch = _ops.batch
batch_toml = _ops.batch_toml
batch_file = _ops.batch_file
parse_batch_toml = _ops.parse_batch_toml
capabilities = _ops.capabilities
memory_key = _ops.memory_key
output_format_for = _ops.output_format_for
parse_args = _ops.parse_args
inspect_request = _ops.inspect_request

DEFAULT_MAX_WORKING_BYTES: int = _ops.DEFAULT_MAX_WORKING_BYTES
DEFAULT_MAX_PENDING_BYTES: int = _ops.DEFAULT_MAX_PENDING_BYTES
DEFAULT_MAX_REPORT_BYTES: int = _ops.DEFAULT_MAX_REPORT_BYTES
DEFAULT_MAX_TASK_REPORT_BYTES: int = _ops.DEFAULT_MAX_TASK_REPORT_BYTES


def vectorize(request=None, /, *, files=None, **options):
    """Vectorize (``fontpixel vectorize``): the same operation as :func:`convert`.

    The command-line ``vectorize`` is an alias of ``convert``; the result is
    identical, including its JSON and text documents.
    """
    return convert(request, files=files, **options)


__all__ = [
    "OperationResult", "BatchResult", "convert", "vectorize", "merge", "variants",
    "subset", "charset", "render", "validate", "diff", "info", "preview",
    "formats", "batch", "batch_toml", "batch_file", "parse_batch_toml",
    "capabilities", "memory_key", "output_format_for", "parse_args", "inspect_request",
    "DEFAULT_MAX_WORKING_BYTES", "DEFAULT_MAX_PENDING_BYTES",
    "DEFAULT_MAX_REPORT_BYTES", "DEFAULT_MAX_TASK_REPORT_BYTES",
]
