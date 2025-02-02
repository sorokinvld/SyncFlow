import AppLogo from '@/app/ui/app-logo';
import { ArrowRightIcon } from '@heroicons/react/24/outline';
import Link from 'next/link';
import { lusitana } from '@/app/ui/fonts';
import getConfig from '@/config';

export default function Page() {
  const deploymentConfig = getConfig();
  return (
    <main className="flex h-full flex-col p-6">
      <div className="m-20 flex h-20 shrink-0 items-end rounded-lg p-4 md:h-64">
        <AppLogo />
      </div>
      <div className="mt-4 flex grow flex-col gap-4 md:flex-row">
        <div className="flex flex-col justify-center gap-6 rounded-lg px-6 py-10 md:w-2/5 md:px-20">
          <p
            className={`${lusitana.className} text-xl md:text-3xl md:leading-normal`}
            dangerouslySetInnerHTML={{ __html: deploymentConfig.tagLine }}
          />
          <Link
            href="/login"
            className="flex items-center gap-5 self-start rounded-lg bg-teal-500 px-6 py-3 text-sm font-medium text-white transition-colors hover:bg-blue-400 md:text-base"
          >
            <span>Log in</span> <ArrowRightIcon className="w-5 md:w-6" />
          </Link>
        </div>
        <div className="flex items-center justify-center p-6 md:w-3/5 md:px-28 md:py-12">
          {/*.
            ToDo: Add a video/image here.
          */}
        </div>
      </div>
    </main>
  );
}
