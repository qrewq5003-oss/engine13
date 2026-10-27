"""base.py on synthetic shapes: holes are found locally, so what lies far away does not change
them (plan §15a п. 2), and a unit crossing the edge of the bbox keeps its parts inside."""

import numpy as np
import shapely

import base


def holes(units, land, bbox):
    ids = np.array([f"u{k}" for k in range(len(units))])
    return base.source_holes(np.array(units, dtype=object), ids, np.array(land, dtype=object),
                             np.array([], dtype=object), bbox, 1.0, 2.0)


def test_a_hole_cut_by_cell_edges_is_joined_back():
    # Land 0..4 × 0..2, units cover all but 1.5..2.5: one hole across the cell edge x = 2.
    got = holes([shapely.box(0, 0, 1.5, 2), shapely.box(2.5, 0, 4, 2)], [shapely.box(0, 0, 4, 2)], (0, 0, 4, 2))
    assert len(got) == 1
    assert shapely.normalize(got[0]).equals(shapely.box(1.5, 0, 2.5, 2))


def test_holes_do_not_depend_on_what_lies_far_away():
    near = [shapely.box(0, 0, 1.5, 2), shapely.box(2.5, 0, 4, 2)]
    a = holes(near, [shapely.box(0, 0, 4, 2)], (0, 0, 4, 2))
    # A wider bbox: more land to the east, a unit covering it, and a hole of its own far off.
    b = holes(near + [shapely.box(4, 0, 9, 2)], [shapely.box(0, 0, 10, 2)], (0, 0, 10, 2))
    assert len(b) == 2
    assert shapely.to_wkb(shapely.normalize(a[0])) == shapely.to_wkb(shapely.normalize(b[0]))


def test_a_unit_across_the_edge_keeps_its_parts_inside():
    bbox = (0, 0, 10, 10)
    inside, outside = shapely.box(1, 1, 2, 2), shapely.box(20, 20, 30, 30)
    # Its point on the surface lies on the bigger part, outside; the part inside is still taken.
    got = base.in_bbox(shapely.MultiPolygon([inside, outside]), bbox)
    assert got.equals(inside)
    # A unit wholly inside is taken as it is, one wholly outside is left out.
    whole = shapely.MultiPolygon([inside, shapely.box(3, 3, 4, 4)])
    assert base.in_bbox(whole, bbox) is whole
    assert base.in_bbox(outside, bbox) is None
