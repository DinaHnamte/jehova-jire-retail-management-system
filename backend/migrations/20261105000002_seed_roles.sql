-- Permissions and roles. Code checks permission codes, never role names (Architecture.md §3.1).

INSERT INTO permissions (code, description) VALUES
  ('products.read',            'View products'),
  ('products.create',          'Create products'),
  ('products.update',          'Edit products and prices'),
  ('products.archive',         'Archive products'),
  ('inventory.read',           'View stock and movements'),
  ('inventory.receive',        'Receive goods into stock'),
  ('inventory.adjust',         'Adjust stock with a reason'),
  ('inventory.count',          'Record stock counts'),
  ('purchases.read',           'View purchase orders and receipts'),
  ('purchases.create',         'Create purchase orders'),
  ('purchases.receive',        'Confirm goods receipts'),
  ('orders.read',              'View orders'),
  ('orders.create',            'Create and edit draft orders'),
  ('orders.confirm',           'Confirm orders and take counter payment'),
  ('orders.void',              'Void completed orders'),
  ('orders.discount_override', 'Give discounts above the normal limit'),
  ('payments.record',          'Record customer payments'),
  ('payments.verify',          'Verify UPI/bank payments'),
  ('payments.reverse',         'Reverse payments'),
  ('returns.create',           'Process returns and refunds'),
  ('reports.read',             'View reports'),
  ('audit.read',               'View the audit log'),
  ('users.manage',             'Manage users and roles'),
  ('settings.manage',          'Change business settings'),
  ('deliveries.read',          'View all deliveries'),
  ('deliveries.assign',        'Assign drivers and edit arrangements'),
  ('deliveries.cancel',        'Cancel deliveries'),
  ('deliveries.check_in',      'Check returned delivery goods back in'),
  ('deliveries.read_own',      'View own assigned deliveries'),
  ('deliveries.update_own',    'Update status of own deliveries'),
  ('deliveries.collect_own',   'Record collections on own deliveries'),
  ('settlements.read',         'View driver settlements'),
  ('settlements.receive',      'Record cash handed over by drivers'),
  ('settlements.pay',          'Pay drivers'),
  ('settlements.read_own',     'View own settlement summary'),
  ('drivers.manage',           'Manage driver profiles');

-- Fixed UUIDs so roles are identical in every environment.
INSERT INTO roles (id, code, name) VALUES
  ('00000000-0000-7000-8000-000000000001', 'admin',           'Administrator'),
  ('00000000-0000-7000-8000-000000000002', 'manager',         'Manager'),
  ('00000000-0000-7000-8000-000000000003', 'cashier',         'Cashier'),
  ('00000000-0000-7000-8000-000000000004', 'stock_operator',  'Stock operator'),
  ('00000000-0000-7000-8000-000000000005', 'delivery_driver', 'Delivery driver'),
  ('00000000-0000-7000-8000-000000000006', 'auditor',         'Auditor');

-- admin: everything
INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000001', code FROM permissions;

-- manager: everything except user/settings management and driver self-service
INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000002', code FROM permissions
WHERE code NOT IN ('users.manage', 'settings.manage') AND code NOT LIKE '%\_own';

INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000003', unnest(ARRAY[
  'products.read', 'inventory.read', 'orders.read', 'orders.create', 'orders.confirm',
  'payments.record', 'returns.create', 'deliveries.read', 'settlements.receive']);

INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000004', unnest(ARRAY[
  'products.read', 'products.create', 'products.update', 'inventory.read',
  'inventory.receive', 'inventory.count', 'purchases.read', 'purchases.receive']);

INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000005', unnest(ARRAY[
  'deliveries.read_own', 'deliveries.update_own', 'deliveries.collect_own', 'settlements.read_own']);

-- auditor: every *.read (not *_own) plus audit.read
INSERT INTO role_permissions (role_id, permission_code)
SELECT '00000000-0000-7000-8000-000000000006', code FROM permissions
WHERE code LIKE '%.read';
