(function() {var implementors = {};
implementors["corundum"] = [{"text":"impl UnwindSafe for BuddyAlloc","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; UnwindSafe for BuddyAlg&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; UnwindSafe for Zones&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl UnwindSafe for Heap","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for Ptr&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for Pbox&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'b, T:&nbsp;?Sized, A&gt; UnwindSafe for Ref&lt;'b, T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'b, T, A&gt; !UnwindSafe for RefMut&lt;'b, T, A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; UnwindSafe for VCell&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe + UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for Prc&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for Weak&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, T, A&gt; !UnwindSafe for MutexGuard&lt;'a, T, A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for ParcInner&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; UnwindSafe for Weak&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; UnwindSafe for FatPtr&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized&gt; UnwindSafe for NonNull&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; !UnwindSafe for LogNonNull&lt;T, A&gt;","synthetic":true,"types":[]},{"text":"impl UnwindSafe for LogEnum","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; UnwindSafe for Notifier&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; UnwindSafe for Log&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; UnwindSafe for String&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; UnwindSafe for Vec&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; UnwindSafe for IntoIteratorHelper&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe + UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, T&gt; UnwindSafe for IterHelper&lt;'a, T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; UnwindSafe for Measure&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: UnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; UnwindSafe for PCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; UnwindSafe for PRefCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T, A:&nbsp;MemPool&gt; UnwindSafe for RootCell&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; UnwindSafe for PrcBox&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; UnwindSafe for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T, A:&nbsp;MemPool&gt; UnwindSafe for PMutex&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; UnwindSafe for Parc&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; UnwindSafe for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl UnwindSafe for Chaperon","synthetic":false,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool&gt; !UnwindSafe for Journal&lt;A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;LooseTxInUnsafe&gt; UnwindSafe for AssertTxInSafe&lt;T&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()