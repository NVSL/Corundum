(function() {var implementors = {};
implementors["corundum"] = [{"text":"impl&lt;A&gt; RefUnwindSafe for BuddyAlg&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; RefUnwindSafe for Zones&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl RefUnwindSafe for Heap","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for Pbox&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'b, T:&nbsp;?Sized, A&gt; RefUnwindSafe for Ref&lt;'b, T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'b, T, A&gt; !RefUnwindSafe for RefMut&lt;'b, T, A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; RefUnwindSafe for VCell&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for Prc&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for Weak&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, T, A&gt; !RefUnwindSafe for MutexGuard&lt;'a, T, A&gt;","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for ParcInner&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for Weak&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; RefUnwindSafe for FatPtr&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A&gt; RefUnwindSafe for Ptr&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized&gt; RefUnwindSafe for NonNull&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; !RefUnwindSafe for LogNonNull&lt;T, A&gt;","synthetic":true,"types":[]},{"text":"impl RefUnwindSafe for LogEnum","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; RefUnwindSafe for Notifier&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; RefUnwindSafe for Log&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; RefUnwindSafe for String&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T, A&gt; RefUnwindSafe for Vec&lt;T, A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;T&gt; RefUnwindSafe for IntoIteratorHelper&lt;T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;'a, T&gt; RefUnwindSafe for IterHelper&lt;'a, T&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;T: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl&lt;A&gt; RefUnwindSafe for Measure&lt;A&gt; <span class=\"where fmt-newline\">where<br>&nbsp;&nbsp;&nbsp;&nbsp;A: RefUnwindSafe,&nbsp;</span>","synthetic":true,"types":[]},{"text":"impl RefUnwindSafe for BuddyAlloc","synthetic":true,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for PCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for PRefCell&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T, A:&nbsp;MemPool&gt; RefUnwindSafe for RootCell&lt;'_, T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for PrcBox&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T, A:&nbsp;MemPool&gt; RefUnwindSafe for PMutex&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;PSafe + ?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for Parc&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;?Sized, A:&nbsp;MemPool&gt; RefUnwindSafe for VWeak&lt;T, A&gt;","synthetic":false,"types":[]},{"text":"impl RefUnwindSafe for Chaperon","synthetic":false,"types":[]},{"text":"impl&lt;A:&nbsp;MemPool&gt; !RefUnwindSafe for Journal&lt;A&gt;","synthetic":false,"types":[]},{"text":"impl&lt;T:&nbsp;LooseTxInUnsafe&gt; RefUnwindSafe for AssertTxInSafe&lt;T&gt;","synthetic":false,"types":[]}];
if (window.register_implementors) {window.register_implementors(implementors);} else {window.pending_implementors = implementors;}})()