import unittest

from run_plan458_sam_product import parse_stock_transit_rows


class StockTransitReceiptTests(unittest.TestCase):
    def test_counts_only_transit_rows(self):
        body = (
            '<table><tr class="tcell_center"><td>private-id</td></tr>'
            '<tr class="tcell_center"><td>private-peer</td></tr></table>'
        )
        self.assertEqual(parse_stock_transit_rows(body), 2)

    def test_empty_inventory_is_zero(self):
        self.assertEqual(parse_stock_transit_rows("no transit tunnels currently built"), 0)

    def test_unrelated_table_rows_do_not_count(self):
        self.assertEqual(parse_stock_transit_rows('<tr class="other"><td>x</td></tr>'), 0)


if __name__ == "__main__":
    unittest.main()
