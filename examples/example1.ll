; ModuleID = 'main'
source_filename = "main"

define {} @a() {
entry:
  %local_0 = alloca {}, align 1
  %local_1 = alloca {}, align 1
  br label %basic_block_0

basic_block_0:                                    ; preds = %entry
  %call = call {} @a()
  store {} %call, ptr %local_0, align 1
  br label %basic_block_1

basic_block_1:                                    ; preds = %basic_block_0
  %load = load {}, ptr %local_1, align 1
  ret {} %load
}

define i32 @main() {
entry:
  %local_0 = alloca {}, align 1
  %local_1 = alloca {}, align 1
  br label %basic_block_0

basic_block_0:                                    ; preds = %entry
  %call = call {} @a()
  store {} %call, ptr %local_0, align 1
  br label %basic_block_1

basic_block_1:                                    ; preds = %basic_block_0
  ret i32 0
}
