SECTIONS {
	/* Note: avoid using __build_id_len from the executable, it will be
	 * relocated and as a result have a fairly large virtual address offset
	 * added to it */
	PROVIDE_HIDDEN(__build_id_start = LOADADDR(.note.gnu.build-id) + 4*4);
	PROVIDE_HIDDEN(__build_id_end = __build_id_start + SIZEOF(.note.gnu.build-id) - 4*4);
} INSERT AFTER .note.gnu.build-id
