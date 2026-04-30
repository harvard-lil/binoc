"""Verify that the _binoc.pyi stub stays in sync with the compiled extension."""

import ast
from pathlib import Path

import binoc._binoc as _binoc

STUB_PATH = Path(__file__).resolve().parent.parent / 'python' / 'binoc' / '_binoc.pyi'

# Names that exist on every PyO3 class but aren't part of our public API.
PYCLASS_BUILTINS = frozenset(
    {
        '__class__',
        '__delattr__',
        '__dir__',
        '__doc__',
        '__eq__',
        '__format__',
        '__ge__',
        '__getattribute__',
        '__getstate__',
        '__gt__',
        '__hash__',
        '__init_subclass__',
        '__le__',
        '__lt__',
        '__module__',
        '__ne__',
        '__new__',
        '__reduce__',
        '__reduce_ex__',
        '__richcmp__',
        '__setattr__',
        '__sizeof__',
        '__str__',
        '__subclasshook__',
    }
)


def _parse_stub():
    """Parse the .pyi file and return (top_level_names, class_members).

    top_level_names: set of class and function names defined at module level.
    class_members: dict mapping class name -> set of method/property names.
    """
    source = STUB_PATH.read_text()
    tree = ast.parse(source)

    top_level_names = set()
    class_members: dict[str, set[str]] = {}

    for node in ast.iter_child_nodes(tree):
        if isinstance(node, ast.ClassDef):
            top_level_names.add(node.name)
            members = set()
            for item in ast.iter_child_nodes(node):
                if isinstance(item, ast.FunctionDef):
                    members.add(item.name)
                elif isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
                    members.add(item.target.id)
            class_members[node.name] = members
        elif isinstance(node, ast.FunctionDef):
            top_level_names.add(node.name)

    return top_level_names, class_members


def _runtime_public_names():
    """Return the set of public names exported by the compiled _binoc module."""
    return {name for name in dir(_binoc) if not name.startswith('_')}


def _runtime_class_members(cls):
    """Return the set of public + dunder method/property names on a class.

    Filters out inherited object builtins that PyO3 injects automatically.
    """
    members = set()
    for name in dir(cls):
        if name.startswith('_') and not name.startswith('__'):
            continue
        if name in PYCLASS_BUILTINS:
            continue
        members.add(name)
    return members


def test_stub_file_exists():
    assert STUB_PATH.exists(), f'Stub file not found: {STUB_PATH}'


def test_top_level_names_match():
    stub_names, _ = _parse_stub()
    runtime_names = _runtime_public_names()

    missing_from_stub = runtime_names - stub_names
    extra_in_stub = stub_names - runtime_names

    errors = []
    if missing_from_stub:
        errors.append(f'In _binoc but missing from stub: {sorted(missing_from_stub)}')
    if extra_in_stub:
        errors.append(f'In stub but missing from _binoc: {sorted(extra_in_stub)}')

    assert not errors, '\n'.join(errors)


def test_class_members_match():
    _, stub_classes = _parse_stub()
    errors = []

    for class_name, stub_members in sorted(stub_classes.items()):
        cls = getattr(_binoc, class_name, None)
        if cls is None:
            continue

        runtime_members = _runtime_class_members(cls)

        # __init__: the stub declares it but PyO3 wires constructors via __new__.
        # __str__: always exists on Python objects; we keep it in stubs only
        # where Rust defines a custom implementation, but don't require parity.
        ignore = {'__init__', '__str__'}
        stub_members = stub_members - ignore
        runtime_members = runtime_members - ignore

        missing_from_stub = runtime_members - stub_members
        extra_in_stub = stub_members - runtime_members

        if missing_from_stub:
            errors.append(
                f'{class_name}: in runtime but missing from stub: {sorted(missing_from_stub)}'
            )
        if extra_in_stub:
            errors.append(
                f'{class_name}: in stub but missing from runtime: {sorted(extra_in_stub)}'
            )

    assert not errors, '\n'.join(errors)
