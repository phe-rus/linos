// SPDX-License-Identifier: GPL-2.0-only
/* Linos-owned boot completion contract for init and local recovery. */
#include <linux/atomic.h>
#include <linux/capability.h>
#include <linux/errno.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/kobject.h>
#include <linux/string.h>
#include <linux/sysfs.h>

static atomic_t linos_ready = ATOMIC_INIT(0);

static ssize_t boot_state_show(struct kobject *kobj,
			      struct kobj_attribute *attr, char *buf)
{
	return sysfs_emit(buf, "%s\n",
			  atomic_read(&linos_ready) ? "ready" : "starting");
}

static ssize_t boot_state_store(struct kobject *kobj,
			       struct kobj_attribute *attr,
			       const char *buf, size_t count)
{
	if (!capable(CAP_SYS_BOOT))
		return -EPERM;
	if (!sysfs_streq(buf, "ready"))
		return -EINVAL;

	if (atomic_cmpxchg(&linos_ready, 0, 1) == 0)
		pr_info("linos: userspace acknowledged boot readiness\n");
	return count;
}

static struct kobj_attribute linos_boot_attribute =
	__ATTR(boot_state, 0644, boot_state_show, boot_state_store);

static int __init linos_boot_init(void)
{
	struct kobject *linos;
	int ret;

	linos = kobject_create_and_add("linos", kernel_kobj);
	if (!linos)
		return -ENOMEM;

	ret = sysfs_create_file(linos, &linos_boot_attribute.attr);
	if (ret) {
		kobject_put(linos);
		return ret;
	}
	pr_info("linos: boot-state service initialized\n");
	return 0;
}
subsys_initcall(linos_boot_init);
